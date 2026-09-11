/**
 * Debounced, per-session, failure-aware delivery of records to
 * `todo ai upsert --stdin` (ai-feed-spec §5, §6). Pure with respect to the
 * host: the process launcher is injected, so tests run with a fake.
 * @module dsh-todo-bridge/publisher
 */

import { contentKey } from './record.ts'
import type { FeedRecord } from './types.ts'

/** Exit facts of one finished CLI run. */
export interface SpawnOutcome {
  /** Exit code; `null` when the process died from a signal. */
  readonly exitCode: number | null
  /** Captured stderr tail, for the log line. */
  readonly stderr: string
}

/**
 * Start the CLI with `argv` (`argv[0]` is the program) and `stdin` as its
 * whole standard input. Resolves with exit facts; rejects only when the
 * process could not be started at all (missing binary, spawn error).
 */
export type Spawner = (argv: readonly string[], stdin: string) => Promise<SpawnOutcome>

/** The two log levels the publisher uses; `ctx.logger` satisfies it. */
export interface PublisherLogger {
  info(message: string): void
  warn(message: string): void
}

/** Publisher policy. */
export interface PublisherOptions {
  readonly spawn: Spawner
  /** Executable name or absolute path of the ToDo CLI. */
  readonly todoBinary: string
  /** Per-session coalescing window in milliseconds. */
  readonly debounceMs: number
  /** Consecutive failures after which publishing pauses. */
  readonly maxConsecutiveFailures: number
  readonly logger: PublisherLogger
  /**
   * While paused, at most one probe attempt per this window; a probe that
   * succeeds resumes publishing. Omit for 60 s.
   */
  readonly pauseProbeMs?: number
  /** Clock, for tests. Omit for `Date.now`. */
  readonly now?: () => number
}

/** Counters exposed for tests and diagnostics. */
export interface PublisherStats {
  /** Records handed to `schedule`. */
  scheduled: number
  /** CLI runs that exited 0. */
  published: number
  /** Records dropped because their content equalled the last published one. */
  unchanged: number
  /** Records dropped while paused (outside a probe window). */
  dropped: number
  /** CLI runs that failed (non-zero exit, signal, or spawn error). */
  failed: number
  consecutiveFailures: number
  paused: boolean
}

interface Pending {
  record: FeedRecord
  timer: ReturnType<typeof setTimeout>
}

const DEFAULT_PAUSE_PROBE_MS = 60_000
/** Longest stderr excerpt kept in one log line. */
const STDERR_EXCERPT_CHARS = 400

/** Arguments after the program: the contract's stdin path (§5). */
export const UPSERT_ARGS: readonly string[] = Object.freeze(['ai', 'upsert', '--stdin'])

/**
 * One publisher per plugin life. `schedule` coalesces per session to the
 * latest record within `debounceMs`; delivery is serialized per session so
 * an older record can never overtake a newer one; identical content is not
 * re-sent (it would only advance ToDo's `event_at` and clear the user's ack).
 */
export class Publisher {
  private readonly pending = new Map<string, Pending>()
  private readonly inflight = new Map<string, Promise<void>>()
  private readonly lastContent = new Map<string, string>()
  private readonly now: () => number
  private readonly pauseProbeMs: number
  private lastProbeAt = Number.NEGATIVE_INFINITY
  private disposed = false

  readonly stats: PublisherStats = {
    scheduled: 0,
    published: 0,
    unchanged: 0,
    dropped: 0,
    failed: 0,
    consecutiveFailures: 0,
    paused: false,
  }

  constructor(private readonly options: PublisherOptions) {
    if (!Number.isInteger(options.debounceMs) || options.debounceMs < 0) {
      throw new TypeError(`todo-bridge: debounceMs must be a non-negative integer, got ${String(options.debounceMs)}`)
    }
    if (!Number.isInteger(options.maxConsecutiveFailures) || options.maxConsecutiveFailures < 1) {
      throw new TypeError(
        `todo-bridge: maxConsecutiveFailures must be a positive integer, got ${String(options.maxConsecutiveFailures)}`,
      )
    }
    this.now = options.now ?? Date.now
    this.pauseProbeMs = options.pauseProbeMs ?? DEFAULT_PAUSE_PROBE_MS
  }

  /**
   * Queue the newest record for its session. Within the debounce window the
   * previous pending record is replaced; the timer is not extended, so a
   * session that changes continuously still publishes every `debounceMs`.
   * @param record - the complete record to send.
   */
  schedule(record: FeedRecord): void {
    if (this.disposed) return
    this.stats.scheduled += 1
    const id = record.session_id
    const existing = this.pending.get(id)
    if (existing !== undefined) {
      existing.record = record
      return
    }
    const timer = setTimeout(() => { this.fire(id) }, this.options.debounceMs)
    // A pending publish must not keep the host process alive on its own.
    timer.unref?.()
    this.pending.set(id, { record, timer })
  }

  /**
   * Deliver every pending record now and wait for every in-flight run.
   * @returns after the last run settles.
   */
  async flush(): Promise<void> {
    for (const [id, entry] of [...this.pending]) {
      clearTimeout(entry.timer)
      this.fire(id)
    }
    await this.settle()
  }

  /**
   * Stop accepting records, drop pending ones, and wait for in-flight runs.
   * @returns after the last run settles.
   */
  async dispose(): Promise<void> {
    this.disposed = true
    for (const entry of this.pending.values()) clearTimeout(entry.timer)
    this.pending.clear()
    await this.settle()
  }

  private async settle(): Promise<void> {
    while (this.inflight.size > 0) {
      await Promise.all([...this.inflight.values()])
    }
  }

  private fire(id: string): void {
    const entry = this.pending.get(id)
    if (entry === undefined) return
    this.pending.delete(id)
    const previous = this.inflight.get(id) ?? Promise.resolve()
    const run: Promise<void> = previous
      .then(() => this.send(entry.record))
      .catch(() => undefined)
      .finally(() => {
        if (this.inflight.get(id) === run) this.inflight.delete(id)
      })
    this.inflight.set(id, run)
  }

  private async send(record: FeedRecord): Promise<void> {
    const id = record.session_id
    const key = contentKey(record)
    if (this.lastContent.get(id) === key) {
      this.stats.unchanged += 1
      return
    }
    const probing = this.stats.paused
    if (probing) {
      if (this.now() - this.lastProbeAt < this.pauseProbeMs) {
        this.stats.dropped += 1
        return
      }
      this.lastProbeAt = this.now()
    }
    let outcome: SpawnOutcome
    try {
      outcome = await this.options.spawn([this.options.todoBinary, ...UPSERT_ARGS], JSON.stringify(record))
    } catch (error: unknown) {
      this.failure(id, `could not start ${this.options.todoBinary}: ${messageOf(error)}`, probing)
      return
    }
    if (outcome.exitCode === 0) {
      this.stats.published += 1
      this.stats.consecutiveFailures = 0
      this.lastContent.set(id, key)
      if (this.stats.paused) {
        this.stats.paused = false
        this.options.logger.info(`todo-bridge: ${this.options.todoBinary} is reachable again; publishing resumed`)
      }
      return
    }
    const exit = outcome.exitCode === null ? 'died from a signal' : `exited ${String(outcome.exitCode)}`
    this.failure(id, `${this.options.todoBinary} ai upsert ${exit}${excerpt(outcome.stderr)}`, probing)
  }

  private failure(id: string, reason: string, probing: boolean): void {
    this.stats.failed += 1
    this.stats.consecutiveFailures += 1
    if (probing) {
      this.options.logger.info(`todo-bridge: probe for session ${id} failed; still paused: ${reason}`)
      return
    }
    this.options.logger.warn(`todo-bridge: publish for session ${id} failed: ${reason}`)
    if (this.stats.consecutiveFailures >= this.options.maxConsecutiveFailures) {
      this.stats.paused = true
      this.lastProbeAt = this.now()
      this.options.logger.warn(
        `todo-bridge: ${String(this.stats.consecutiveFailures)} consecutive failures; publishing paused `
        + `(one probe every ${String(Math.round(this.pauseProbeMs / 1000))} s until ${this.options.todoBinary} succeeds)`,
      )
    }
  }
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function excerpt(stderr: string): string {
  const text = stderr.trim().replace(/\s+/gu, ' ')
  if (text.length === 0) return ''
  return `: ${text.length > STDERR_EXCERPT_CHARS ? `${text.slice(0, STDERR_EXCERPT_CHARS)}…` : text}`
}
