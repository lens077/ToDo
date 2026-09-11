/**
 * dsh-todo-bridge: a DeepSeek Harness host plugin that publishes the digest
 * panel's per-session state to the ToDo CLI (`todo ai upsert --stdin`,
 * ai-feed-spec 1). It joins the `sessionDigest` projection with the durable
 * inbox marks and the live agent/interaction signals, classifies each
 * session with the panel's own rules (`classify.ts`), builds one contract
 * record (`record.ts`), and hands it to a debounced, failure-aware
 * publisher (`publisher.ts`).
 *
 * Install (bundle): `dsh plugin --profile web add file:/abs/path/to/dsh-todo-bridge`.
 * The bundle patch mounts this plugin as the `todo-bridge` row.
 *
 * @module dsh-todo-bridge
 */

import { realpathSync } from 'node:fs'
import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type { Agent } from '@deepseek-ai/dsh-agent/types'
import type {} from '@deepseek-ai/dsh-agent'
import type { Session, SessionHeader, SessionId, SessionLogOffset } from '@deepseek-ai/dsh-session'
import type {} from '@deepseek-ai/dsh-session-projection'
import type { ProjectionSnapshot } from '@deepseek-ai/dsh-session-projection'
import type {} from '@deepseek-ai/dsh-session-digest/types'
import type {} from '@deepseek-ai/dsh-session-title/types'
import type {} from '@deepseek-ai/dsh-session-inbox'
import type { InboxSessionState, InboxSnapshot } from '@deepseek-ai/dsh-session-inbox/types'
import type {} from '@deepseek-ai/dsh-session-projection-cache'
import type {} from '@deepseek-ai/dsh-session-query'
import type {} from '@deepseek-ai/dsh-workspace'
import type {} from '@deepseek-ai/dsh-host-webserver'
import type {} from '@deepseek-ai/dsh-subprocess'
import type {} from '@deepseek-ai/dsh-user-approval'
import type {} from '@deepseek-ai/dsh-user-questions'
import type { DomainChanged } from '@deepseek-ai/dsh-storage-domain'
import { classify } from './classify.ts'
import { buildRecord } from './record.ts'
import { Publisher } from './publisher.ts'
import { createNodeSpawner, createSubprocessSpawner } from './spawn.ts'
import type { BridgeDigest, BridgeInboxRow, BridgeSessionMeta } from './types.ts'

export type * from './types.ts'
export { classify, classifyCategory, eligible, landedSeq, toFeedState } from './classify.ts'
export { buildRecord, contentKey, eventAtOf, relativeToWorkspace, truncateUtf8, CONTRACT_LIMITS } from './record.ts'
export { Publisher, UPSERT_ARGS } from './publisher.ts'
export type { PublisherLogger, PublisherOptions, PublisherStats, SpawnOutcome, Spawner } from './publisher.ts'
export { createNodeSpawner, createSubprocessSpawner } from './spawn.ts'

/** Cordis function-plugin name. */
export const name = 'todo-bridge'
/** Required services; the plugin stays pending until every one is available. */
export const inject = ['sessionProjections', 'sessionInbox', 'sessions', 'agents']

/** Plugin configuration. */
export interface Config {
  /** Publish at all. Omit for `true`. */
  enabled?: boolean
  /** ToDo CLI executable: a PATH name or an absolute path. Omit for `todo`. */
  todoBinary?: string
  /** Absolute workspace roots to publish (a session under any of them qualifies); empty publishes every workspace. */
  workspaces?: string[]
  /** Per-session coalescing window in milliseconds. Omit for 300. */
  debounceMs?: number
  /** Consecutive CLI failures after which publishing pauses. Omit for 5. */
  maxConsecutiveFailures?: number
  /**
   * `open_url` template. `{base}` is the Web server's loopback URL and
   * `{sessionId}` the URL-encoded session id. Omit for
   * `{base}/?session={sessionId}`, the deep link this bundle's client half
   * (`lib/client.js`) resolves in the Web GUI.
   */
  openUrlTemplate?: string
}

/** Default `open_url` template: the deep link the bundle's client half handles. */
export const DEFAULT_OPEN_URL_TEMPLATE = '{base}/?session={sessionId}'

/** Schemastery validation for {@link Config}. */
export const Config: z<Config> = z.object({
  enabled: z.boolean().default(true),
  todoBinary: z.string().default('todo'),
  workspaces: z.array(z.string()).default([]),
  debounceMs: z.natural().default(300),
  maxConsecutiveFailures: z.natural().min(1).default(5),
  openUrlTemplate: z.string().default(DEFAULT_OPEN_URL_TEMPLATE),
})

/** Loopback URL when no Web server is mounted (matches the shipped `dsh web` default port). */
const FALLBACK_BASE_URL = 'http://127.0.0.1:3080'
const LOOPBACK_HOST = '127.0.0.1'

/** Projection keys the bridge reads in one consistent cut. */
const PROJECTION_KEYS = ['sessionDigest', 'title', 'sessionListMetadata'] as const

/** Per-session runtime signals the host does not persist. */
interface Signals {
  /** Pending approvals and questions; `waiting` while positive. */
  pendingInteractions: number
  /** Time of the newest observed signal change, epoch ms. */
  signalAt: number | null
}

/**
 * Mount the bridge.
 * @param ctx - host context carrying the injected services.
 * @param config - validated configuration.
 */
export function apply(ctx: Context, config: Config = {}): void {
  const settings = {
    enabled: config.enabled ?? true,
    todoBinary: config.todoBinary ?? 'todo',
    workspaces: (config.workspaces ?? []).map(normalizeRoot),
    debounceMs: config.debounceMs ?? 300,
    maxConsecutiveFailures: config.maxConsecutiveFailures ?? 5,
    openUrlTemplate: config.openUrlTemplate ?? DEFAULT_OPEN_URL_TEMPLATE,
  }
  if (!settings.enabled) {
    ctx.logger.info('todo-bridge: disabled by configuration')
    return
  }

  const subprocess = ctx.get('subprocess')
  const spawner = subprocess === undefined ? createNodeSpawner() : createSubprocessSpawner(subprocess)
  const publisher = new Publisher({
    spawn: spawner,
    todoBinary: settings.todoBinary,
    debounceMs: settings.debounceMs,
    maxConsecutiveFailures: settings.maxConsecutiveFailures,
    logger: ctx.logger,
  })
  ctx.effect(() => async () => { await publisher.dispose() }, 'todo-bridge.publisher')

  const signals = new Map<SessionId, Signals>()
  const signalsOf = (id: SessionId): Signals => {
    let entry = signals.get(id)
    if (entry === undefined) {
      entry = { pendingInteractions: 0, signalAt: null }
      signals.set(id, entry)
    }
    return entry
  }
  const touch = (id: SessionId): void => { signalsOf(id).signalAt = Date.now() }

  let inboxRows = indexInbox(ctx.sessionInbox.get())
  let archived = new Set<string>(ctx.get('workspaceRegistry')?.archivedSessionIds ?? [])

  const openUrlOf = (sessionId: SessionId): string => {
    const port = ctx.get('webServer')?.port
    const base = port === undefined ? FALLBACK_BASE_URL : `http://${LOOPBACK_HOST}:${String(port)}`
    return settings.openUrlTemplate
      .replaceAll('{base}', base)
      .replaceAll('{sessionId}', encodeURIComponent(sessionId))
  }

  const workspaceAllowed = (workspace: string): boolean =>
    settings.workspaces.length === 0 || settings.workspaces.some(root => isWithin(workspace, root))

  // `header.cwd` may be a symlinked spelling of the directory the mutation
  // tools recorded (or the reverse); the realpath is the second prefix
  // `relativeToWorkspace` tries. Resolved once per workspace per plugin life.
  const aliasCache = new Map<string, readonly string[]>()
  const aliasesOf = (workspace: string): readonly string[] => {
    let aliases = aliasCache.get(workspace)
    if (aliases === undefined) {
      aliases = workspaceAliases(workspace)
      aliasCache.set(workspace, aliases)
    }
    return aliases
  }

  /** Classify and, when publishable, hand a record to the publisher. */
  const publish = (
    meta: BridgeSessionMeta,
    digest: BridgeDigest | undefined,
    row: BridgeInboxRow | undefined,
  ): void => {
    if (meta.workspace === null || !workspaceAllowed(meta.workspace)) return
    const state = classify(digest, row, meta, Date.now())
    if (state === null) return
    publisher.schedule(buildRecord({
      meta,
      digest,
      row,
      state,
      openUrl: openUrlOf(meta.sessionId as SessionId),
      workspaceAliases: aliasesOf(meta.workspace),
    }))
  }

  /** Evaluate one attached session from its live projection cut. */
  const evaluateLive = (session: Session): void => {
    let cut: ProjectionSnapshot
    try {
      cut = ctx.sessionProjections.snapshot(session, PROJECTION_KEYS as unknown as Parameters<typeof ctx.sessionProjections.snapshot>[1])
    } catch (error) {
      ctx.logger.warn(`todo-bridge: projection read for session "${session.id}" failed: ${String(error)}`)
      return
    }
    const values = readProjectionValues(cut)
    const signal = signals.get(session.id)
    const meta: BridgeSessionMeta = {
      sessionId: session.id,
      workspace: session.header.cwd ?? null,
      title: values.title,
      createdAt: session.header.createdAt,
      running: ctx.agents.get(session.id)?.status === 'running',
      waiting: (signal?.pendingInteractions ?? 0) > 0,
      blank: values.blank ?? session.seq === 0,
      subagent: session.header.origin === 'subagent',
      archived: archived.has(session.id),
      signalAt: signal?.signalAt ?? null,
    }
    publish(meta, values.digest, inboxRows.get(session.id))
  }

  /** Evaluate one persisted-but-detached session from the projection cache. */
  const evaluateCold = (header: SessionHeader): void => {
    if (header.isSeeded) return
    const cache = ctx.get('sessionProjectionCache')
    if (cache === undefined) return
    let cut: ProjectionSnapshot | undefined
    try {
      cut = cache.cachedSnapshot(header, 0 as SessionLogOffset, PROJECTION_KEYS as unknown as Parameters<typeof cache.cachedSnapshot>[2])
    } catch (error) {
      ctx.logger.warn(`todo-bridge: cached projection read for session "${header.id}" failed: ${String(error)}`)
      return
    }
    if (cut === undefined) return
    const values = readProjectionValues(cut)
    const signal = signals.get(header.id)
    const meta: BridgeSessionMeta = {
      sessionId: header.id,
      workspace: header.cwd ?? null,
      title: values.title,
      createdAt: header.createdAt,
      running: false,
      waiting: false,
      // A cache miss on the metadata unit stays visible, as the session list does.
      blank: values.blank ?? false,
      subagent: header.origin === 'subagent',
      archived: archived.has(header.id),
      signalAt: signal?.signalAt ?? null,
    }
    publish(meta, values.digest, inboxRows.get(header.id))
  }

  /** Evaluate by id: the live session when attached, else the cold path. */
  const evaluateById = async (id: SessionId): Promise<void> => {
    const live = ctx.sessions.get(id)
    if (live !== undefined) {
      evaluateLive(live)
      return
    }
    const query = ctx.get('sessionQuery')
    if (query === undefined) return
    const records = await query.listSessions()
    const record = records.find(entry => entry.header.id === id)
    if (record !== undefined && !record.live) evaluateCold(record.header)
  }

  // Digest, title, and blank-flag changes arrive through the projection feed.
  ctx.sessionProjections.onChanged((session, key) => {
    // `sessionListMetadata` is declared by the session controller, which the
    // bridge does not import; compare as plain strings.
    if ((PROJECTION_KEYS as readonly string[]).includes(key)) evaluateLive(session)
  })

  // Seen / handled / snoozed / pinned marks.
  ctx.on('session-inbox/changed', (snapshot: InboxSnapshot) => {
    const next = indexInbox(snapshot)
    const changed = diffInbox(inboxRows, next)
    inboxRows = next
    for (const id of changed) {
      touch(id)
      void evaluateById(id).catch((error: unknown) => {
        ctx.logger.warn(`todo-bridge: re-evaluation after inbox change for "${id}" failed: ${String(error)}`)
      })
    }
  })

  // Turn start / end.
  ctx.on('agent/status', ({ agent }: { agent: Agent }) => {
    touch(agent.id)
    evaluateLive(agent.session)
  })

  // Pending approvals and user questions: the host-side equivalent of the
  // client's pending-interaction set. Registered ahead of the UI answerers
  // (prepend) and always delegating, so the chain is unchanged.
  const trackInteraction = async <T>(agent: Agent | undefined, next: () => Promise<T>): Promise<T> => {
    if (agent === undefined) return next()
    const entry = signalsOf(agent.id)
    entry.pendingInteractions += 1
    entry.signalAt = Date.now()
    evaluateLive(agent.session)
    try {
      return await next()
    } finally {
      entry.pendingInteractions = Math.max(0, entry.pendingInteractions - 1)
      entry.signalAt = Date.now()
      // The agent may already be gone (cancelled turn); a disposed session
      // is simply not re-evaluated.
      if (ctx.sessions.get(agent.id) !== undefined) evaluateLive(agent.session)
    }
  }
  ctx.on('approval/request', (request, next) => trackInteraction(request.agent, next), true)
  ctx.on('user-questions/request', (request, next) => trackInteraction(request.agent, next), true)

  // Archive set changes ride the workspace domain's global-singleton write.
  ctx.on('domain/changed', (change: DomainChanged) => {
    if (change.domain !== 'workspace' || change.table !== '') return
    const next = new Set<string>(ctx.get('workspaceRegistry')?.archivedSessionIds ?? [])
    const toggled = symmetricDifference(archived, next)
    archived = next
    for (const id of toggled) {
      touch(id as SessionId)
      void evaluateById(id as SessionId).catch((error: unknown) => {
        ctx.logger.warn(`todo-bridge: re-evaluation after archive change for "${id}" failed: ${String(error)}`)
      })
    }
  })

  // A session attached later (resume) folds its projections on first read.
  ctx.on('session/created', (session: Session) => { evaluateLive(session) })

  // Initial snapshot: every attached session, then every persisted one the
  // query seam lists (cold rows read the projection cache, as the session
  // list does). A session whose turn is open but has no running agent was
  // interrupted before this plugin life; its observation time is now, so
  // the resulting `failed` record can advance ToDo's `event_at`.
  const startedAt = Date.now()
  const initial = async (): Promise<void> => {
    const seen = new Set<SessionId>()
    for (const session of ctx.sessions.list()) {
      seen.add(session.id)
      markInterrupted(session, startedAt)
      evaluateLive(session)
    }
    const query = ctx.get('sessionQuery')
    if (query === undefined) return
    const records = await query.listSessions()
    for (const record of records) {
      if (seen.has(record.header.id)) continue
      const live = ctx.sessions.get(record.header.id)
      if (live !== undefined) {
        seen.add(live.id)
        markInterrupted(live, startedAt)
        evaluateLive(live)
        continue
      }
      if (record.header.cwd === undefined) continue
      evaluateCold(record.header)
    }
  }
  const markInterrupted = (session: Session, at: number): void => {
    if (ctx.agents.get(session.id)?.status === 'running') return
    let digest: BridgeDigest | undefined
    try {
      digest = readProjectionValues(ctx.sessionProjections.snapshot(session, ['sessionDigest'])).digest
    } catch {
      return
    }
    if (digest !== undefined && digest.question !== null && digest.outcome === null) {
      signalsOf(session.id).signalAt = at
    }
  }
  void initial().catch((error: unknown) => {
    ctx.logger.warn(`todo-bridge: initial snapshot failed: ${String(error)}`)
  })
  ctx.logger.info(
    `todo-bridge: publishing to "${settings.todoBinary}" via ${subprocess === undefined ? 'node:child_process' : 'ctx.subprocess'}`
    + `${settings.workspaces.length === 0 ? '' : ` for ${String(settings.workspaces.length)} workspace root(s)`}`,
  )
}

/** The projection values the bridge reads, validated structurally. */
interface ProjectionValues {
  readonly digest: BridgeDigest | undefined
  readonly title: string | null
  readonly blank: boolean | undefined
}

function readProjectionValues(cut: ProjectionSnapshot): ProjectionValues {
  const values = cut.values as Record<string, unknown>
  const title = values.title
  const metadata = values.sessionListMetadata
  return {
    digest: asDigest(values.sessionDigest),
    title: typeof title === 'string' ? title : null,
    blank: isRecord(metadata) && typeof metadata.blank === 'boolean' ? metadata.blank : undefined,
  }
}

const OUTCOMES = new Set(['completed', 'error', 'aborted', 'blocked', 'max-tokens', 'interrupted'])

function asDigest(value: unknown): BridgeDigest | undefined {
  if (!isRecord(value)) return undefined
  const outcome = value.outcome
  return {
    question: typeof value.question === 'string' ? value.question : null,
    questionTruncated: value.questionTruncated === true,
    questionSeq: numberOrNull(value.questionSeq),
    questionAt: numberOrNull(value.questionAt),
    reply: typeof value.reply === 'string' ? value.reply : null,
    replyTruncated: value.replyTruncated === true,
    outcome: typeof outcome === 'string' && OUTCOMES.has(outcome) ? outcome as BridgeDigest['outcome'] : null,
    replySeq: numberOrNull(value.replySeq),
    repliedAt: numberOrNull(value.repliedAt),
    changedFiles: Array.isArray(value.changedFiles) ? value.changedFiles.filter((file): file is string => typeof file === 'string') : [],
    changedFileCount: typeof value.changedFileCount === 'number' ? value.changedFileCount : 0,
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function indexInbox(snapshot: InboxSnapshot): Map<SessionId, InboxSessionState> {
  const rows = new Map<SessionId, InboxSessionState>()
  for (const row of snapshot.sessions) rows.set(row.sessionId, row)
  return rows
}

/** Session ids whose row appeared, disappeared, or changed. */
function diffInbox(
  before: ReadonlyMap<SessionId, InboxSessionState>,
  after: ReadonlyMap<SessionId, InboxSessionState>,
): SessionId[] {
  const changed: SessionId[] = []
  for (const [id, row] of after) {
    const previous = before.get(id)
    if (previous === undefined || !sameRow(previous, row)) changed.push(id)
  }
  for (const id of before.keys()) {
    if (!after.has(id)) changed.push(id)
  }
  return changed
}

function sameRow(left: InboxSessionState, right: InboxSessionState): boolean {
  return left.lastSeenSeq === right.lastSeenSeq
    && left.handledAt === right.handledAt
    && left.snoozedUntil === right.snoozedUntil
    && left.pinned === right.pinned
    && left.updatedAt === right.updatedAt
}

function symmetricDifference(left: ReadonlySet<string>, right: ReadonlySet<string>): string[] {
  const result: string[] = []
  for (const id of left) if (!right.has(id)) result.push(id)
  for (const id of right) if (!left.has(id)) result.push(id)
  return result
}

/**
 * Other absolute spellings of a workspace directory: its realpath when that
 * differs (symlinked home, `/private/tmp`, ...). A missing directory has none.
 * @param workspace - absolute session workspace.
 * @returns zero or one alias.
 */
export function workspaceAliases(workspace: string): readonly string[] {
  try {
    const real = realpathSync.native(workspace)
    return real === workspace ? [] : [real]
  } catch {
    return []
  }
}

/** Strip a trailing separator so prefix matching is exact. */
function normalizeRoot(root: string): string {
  return root.length > 1 ? root.replace(/[\\/]+$/u, '') : root
}

/** Whether `path` equals `root` or lies under it. */
function isWithin(path: string, root: string): boolean {
  if (path === root) return true
  const prefix = root.endsWith('/') || root.endsWith('\\') ? root : `${root}/`
  return path.startsWith(prefix) || path.startsWith(`${root}\\`)
}
