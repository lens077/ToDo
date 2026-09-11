/**
 * Wiring test of the Cordis plugin against a small fake of the host context:
 * projection feed, inbox feed, agent status, approval / question waterfalls,
 * archive toggles, the initial snapshot, and the `ctx.subprocess` launcher.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apply, Config } from '../src/index.ts'
import type { FeedRecord } from '../src/types.ts'
import { NOW, WORKSPACE, digest, openDigest } from './fixtures.ts'

type Listener = (...args: unknown[]) => unknown

interface FakeSession {
  id: string
  header: { id: string; createdAt: number; cwd?: string | undefined; origin?: 'subagent'; isSeeded: boolean }
  seq: number
}

interface FakeHost {
  ctx: Record<string, unknown>
  sessions: Map<string, FakeSession>
  projections: Map<string, Record<string, unknown>>
  inbox: { sessions: Array<Record<string, unknown>>; todos: never[]; reviewedAt: null }
  agents: Map<string, { id: string; session: FakeSession; status: 'idle' | 'running' }>
  archived: string[]
  listeners: Map<string, Listener[]>
  projectionListeners: Listener[]
  disposers: Array<() => unknown>
  calls: Array<{ argv: readonly string[]; record: FeedRecord }>
  logs: string[]
  emit(name: string, ...args: unknown[]): unknown
  waterfall(name: string, request: unknown, inner: () => Promise<unknown>): Promise<unknown>
  projectionChanged(sessionId: string, key: string): void
}

function session(id: string, overrides: Partial<FakeSession['header']> = {}, seq = 20): FakeSession {
  return { id, header: { id, createdAt: NOW - 120_000, cwd: WORKSPACE, isSeeded: false, ...overrides }, seq }
}

function makeHost(): FakeHost {
  const listeners = new Map<string, Listener[]>()
  const projectionListeners: Listener[] = []
  const disposers: Array<() => unknown> = []
  const calls: FakeHost['calls'] = []
  const logs: string[] = []
  const host: FakeHost = {
    sessions: new Map(),
    projections: new Map(),
    inbox: { sessions: [], todos: [], reviewedAt: null },
    agents: new Map(),
    archived: [],
    listeners,
    projectionListeners,
    disposers,
    calls,
    logs,
    ctx: {},
    emit(name, ...args) {
      for (const listener of listeners.get(name) ?? []) listener(...args)
      return undefined
    },
    async waterfall(name, request, inner) {
      const chain = [...(listeners.get(name) ?? [])]
      const run = (index: number): Promise<unknown> => {
        const listener = chain[index]
        if (listener === undefined) return inner()
        return Promise.resolve(listener(request, () => run(index + 1)))
      }
      return run(0)
    },
    projectionChanged(sessionId, key) {
      const live = host.sessions.get(sessionId)
      if (live === undefined) throw new Error(`no live session ${sessionId}`)
      for (const listener of projectionListeners) listener(live, key, host.projections.get(sessionId)?.[key], live.seq)
    },
  }
  const subprocess = {
    resolveExecutable: async (command: string) => `/resolved/${command}`,
    spawn: (spec: { argv: readonly string[]; stdio: { stdin: { data: string } } }) => {
      calls.push({ argv: spec.argv, record: JSON.parse(spec.stdio.stdin.data) as FeedRecord })
      return {
        done: Promise.resolve({ exitCode: 0, signal: null }),
        collected: { stderr: { readFrom: () => ({ text: '', nextOffset: 0, lossy: false }) } },
      }
    },
  }
  const services: Record<string, unknown> = {
    subprocess,
    webServer: { port: 4321 },
    workspaceRegistry: { get archivedSessionIds() { return host.archived } },
  }
  host.ctx = {
    logger: {
      info: (message: string) => { logs.push(`info ${message}`) },
      warn: (message: string) => { logs.push(`warn ${message}`) },
    },
    get: (name: string) => services[name],
    on: (name: string, listener: Listener, options?: boolean | { prepend?: boolean }) => {
      const bucket = listeners.get(name) ?? []
      const prepend = options === true || (typeof options === 'object' && options.prepend === true)
      if (prepend) bucket.unshift(listener)
      else bucket.push(listener)
      listeners.set(name, bucket)
      return () => true
    },
    effect: (execute: () => () => unknown) => {
      disposers.push(execute())
      return () => undefined
    },
    sessionProjections: {
      snapshot: (live: FakeSession) => ({ asOfSeq: live.seq - 1, values: host.projections.get(live.id) ?? {} }),
      onChanged: (listener: Listener) => {
        projectionListeners.push(listener)
        return () => undefined
      },
    },
    sessionInbox: { get: () => host.inbox },
    sessions: {
      list: () => [...host.sessions.values()],
      get: (id: string) => host.sessions.get(id),
    },
    agents: { get: (id: string) => host.agents.get(id) },
  }
  return host
}

function mount(host: FakeHost, config: Record<string, unknown> = {}): void {
  apply(host.ctx as never, new Config(config) as never)
}

async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(300)
  await vi.advanceTimersByTimeAsync(0)
}

describe('todo-bridge plugin', () => {
  beforeEach(() => { vi.useFakeTimers({ now: NOW }) })
  afterEach(() => { vi.useRealTimers() })

  it('Config applies the documented defaults', () => {
    expect(new Config({})).toMatchObject({
      enabled: true, todoBinary: 'todo', workspaces: [], debounceMs: 300, maxConsecutiveFailures: 5, openUrlTemplate: '{base}',
    })
  })

  it('publishes an initial snapshot of every eligible attached session through ctx.subprocess', async () => {
    const host = makeHost()
    host.sessions.set('a', session('a'))
    host.projections.set('a', { sessionDigest: digest(), title: 'A', sessionListMetadata: { blank: false, lastPromptAt: NOW } })
    host.sessions.set('blank', session('blank', {}, 0))
    host.sessions.set('sub', session('sub', { origin: 'subagent' }))
    host.projections.set('sub', { sessionDigest: digest(), title: 'S' })
    host.sessions.set('archived', session('archived'))
    host.projections.set('archived', { sessionDigest: digest(), title: 'X' })
    host.archived = ['archived']
    host.sessions.set('nocwd', session('nocwd', { cwd: undefined }))
    host.projections.set('nocwd', { sessionDigest: digest(), title: 'N' })
    mount(host)
    await settle()
    expect(host.calls.map(call => call.record.session_id)).toEqual(['a'])
    expect(host.calls[0]?.argv).toEqual(['/resolved/todo', 'ai', 'upsert', '--stdin'])
    expect(host.calls[0]?.record).toMatchObject({
      state: 'unread', title: 'A', open_url: 'http://127.0.0.1:4321', workspace: WORKSPACE, changed_files: ['README.md'],
    })
    expect(host.logs.some(line => line.includes('via ctx.subprocess'))).toBe(true)
  })

  it('honours the workspace allowlist and the openUrlTemplate', async () => {
    const host = makeHost()
    host.sessions.set('in', session('in', { cwd: `${WORKSPACE}/apps/desktop` }))
    host.projections.set('in', { sessionDigest: digest() })
    host.sessions.set('out', session('out', { cwd: '/elsewhere' }))
    host.projections.set('out', { sessionDigest: digest() })
    mount(host, { workspaces: [`${WORKSPACE}/`], openUrlTemplate: '{base}/?session={sessionId}' })
    await settle()
    expect(host.calls.map(call => call.record.session_id)).toEqual(['in'])
    expect(host.calls[0]?.record.open_url).toBe('http://127.0.0.1:4321/?session=in')
  })

  it('does nothing when disabled', async () => {
    const host = makeHost()
    host.sessions.set('a', session('a'))
    host.projections.set('a', { sessionDigest: digest() })
    mount(host, { enabled: false })
    await settle()
    expect(host.calls).toHaveLength(0)
    expect(host.listeners.size).toBe(0)
  })

  it('follows the digest → running → waiting → unread → seen → handled lifecycle', async () => {
    const host = makeHost()
    const live = session('a')
    host.sessions.set('a', live)
    host.projections.set('a', { sessionDigest: openDigest(), title: 'A' })
    const agent: { id: string; session: FakeSession; status: 'idle' | 'running' } = { id: 'a', session: live, status: 'running' }
    host.agents.set('a', agent)
    mount(host)
    await settle()
    expect(host.calls.at(-1)?.record).toMatchObject({ state: 'running', outcome: null })

    // An approval request: waiting while pending, running again once answered.
    let answer: (value: string) => void = () => undefined
    const pending = host.waterfall('approval/request', { agent }, () => new Promise<string>((resolve) => { answer = resolve }))
    await settle()
    expect(host.calls.at(-1)?.record.state).toBe('waiting')
    answer('allowed-once')
    await expect(pending).resolves.toBe('allowed-once')
    await settle()
    expect(host.calls.at(-1)?.record.state).toBe('running')

    // The turn ends: digest closes, the agent goes idle. The idle flip is the
    // newest source-side event, so it is the record's event_at.
    vi.setSystemTime(NOW + 500)
    host.projections.set('a', { sessionDigest: digest(), title: 'A' })
    host.projectionChanged('a', 'sessionDigest')
    agent.status = 'idle'
    host.emit('agent/status', { agent, status: 'idle' })
    await settle()
    expect(host.calls.at(-1)?.record).toMatchObject({ state: 'unread', outcome: 'completed', event_at: NOW + 500 })

    // The user opens it (seen mark), then marks it handled.
    host.inbox = { sessions: [{ sessionId: 'a', lastSeenSeq: 14, handledAt: null, snoozedUntil: null, pinned: false, updatedAt: NOW + 1_000 }], todos: [], reviewedAt: null }
    host.emit('session-inbox/changed', host.inbox)
    await settle()
    expect(host.calls.at(-1)?.record).toMatchObject({ state: 'seen', event_at: NOW + 1_000 })
    host.inbox = { sessions: [{ sessionId: 'a', lastSeenSeq: 14, handledAt: NOW + 2_000, snoozedUntil: null, pinned: false, updatedAt: NOW + 2_000 }], todos: [], reviewedAt: null }
    host.emit('session-inbox/changed', host.inbox)
    await settle()
    expect(host.calls.at(-1)?.record).toMatchObject({ state: 'handled', event_at: NOW + 2_000 })
    expect(host.calls.map(call => call.record.state)).toEqual(['running', 'waiting', 'running', 'unread', 'seen', 'handled'])
  })

  it('does not publish a snoozed session, and re-publishes once the row is cleared with a fresh event_at', async () => {
    const host = makeHost()
    host.sessions.set('a', session('a'))
    host.projections.set('a', { sessionDigest: digest() })
    host.inbox = { sessions: [{ sessionId: 'a', lastSeenSeq: null, handledAt: null, snoozedUntil: NOW + 60_000, pinned: false, updatedAt: NOW }], todos: [], reviewedAt: null }
    mount(host)
    await settle()
    expect(host.calls).toHaveLength(0)
    vi.setSystemTime(NOW + 5_000)
    host.inbox = { sessions: [], todos: [], reviewedAt: null }
    host.emit('session-inbox/changed', host.inbox)
    await settle()
    expect(host.calls.at(-1)?.record).toMatchObject({ state: 'unread', event_at: NOW + 5_000 })
  })

  it('a user question marks the session waiting even when the answerer throws', async () => {
    const host = makeHost()
    const live = session('a')
    host.sessions.set('a', live)
    host.projections.set('a', { sessionDigest: openDigest() })
    const agent = { id: 'a', session: live, status: 'running' as const }
    host.agents.set('a', agent)
    mount(host)
    await settle()
    let fail: (error: Error) => void = () => undefined
    const failing = host.waterfall('user-questions/request', { agent, questions: [] }, () => new Promise((_, reject) => { fail = reject }))
    await settle()
    expect(host.calls.at(-1)?.record.state).toBe('waiting')
    fail(new Error('no answerer'))
    await expect(failing).rejects.toThrow('no answerer')
    await settle()
    expect(host.calls.at(-1)?.record.state).toBe('running')
  })

  it('an interaction that settles inside the debounce window never publishes a waiting record', async () => {
    const host = makeHost()
    const live = session('a')
    host.sessions.set('a', live)
    host.projections.set('a', { sessionDigest: openDigest() })
    const agent = { id: 'a', session: live, status: 'running' as const }
    host.agents.set('a', agent)
    mount(host)
    await settle()
    await expect(host.waterfall('approval/request', { agent }, () => Promise.resolve('allowed-once'))).resolves.toBe('allowed-once')
    await settle()
    expect(host.calls.map(call => call.record.state)).toEqual(['running'])
  })

  it('stops publishing an archived session and resumes when unarchived', async () => {
    const host = makeHost()
    host.sessions.set('a', session('a'))
    host.projections.set('a', { sessionDigest: digest() })
    mount(host)
    await settle()
    expect(host.calls).toHaveLength(1)
    host.archived = ['a']
    host.emit('domain/changed', { domain: 'workspace', table: '', key: '', operation: 'put', value: {} })
    // Changes while archived are not published…
    host.inbox = { sessions: [{ sessionId: 'a', lastSeenSeq: 14, handledAt: null, snoozedUntil: null, pinned: false, updatedAt: NOW + 1_000 }], todos: [], reviewedAt: null }
    host.emit('session-inbox/changed', host.inbox)
    host.projectionChanged('a', 'sessionDigest')
    await settle()
    expect(host.calls).toHaveLength(1)
    // …and unarchiving publishes the current state with the toggle time as event_at.
    vi.setSystemTime(NOW + 10_000)
    host.archived = []
    host.emit('domain/changed', { domain: 'workspace', table: '', key: '', operation: 'put', value: {} })
    await settle()
    expect(host.calls).toHaveLength(2)
    expect(host.calls[1]?.record).toMatchObject({ state: 'seen', event_at: NOW + 10_000 })
  })

  it('does not re-send identical content after a no-op re-evaluation', async () => {
    const host = makeHost()
    host.sessions.set('a', session('a'))
    host.projections.set('a', { sessionDigest: digest() })
    mount(host)
    await settle()
    host.projectionChanged('a', 'title')
    host.emit('session/created', host.sessions.get('a'))
    await settle()
    expect(host.calls).toHaveLength(1)
  })

  it('marks an interrupted open turn with the start time so its failed record advances event_at', async () => {
    const host = makeHost()
    host.sessions.set('a', session('a'))
    host.projections.set('a', { sessionDigest: openDigest() })
    mount(host)
    await settle()
    expect(host.calls.at(-1)?.record).toMatchObject({ state: 'failed', outcome: null, event_at: NOW })
  })

  it('disposes the publisher through the fiber effect', async () => {
    const host = makeHost()
    mount(host)
    expect(host.disposers).toHaveLength(1)
    await expect(Promise.resolve(host.disposers[0]?.())).resolves.toBeUndefined()
  })
})
