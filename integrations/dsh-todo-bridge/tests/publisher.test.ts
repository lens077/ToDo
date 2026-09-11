import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Publisher, UPSERT_ARGS } from '../src/publisher.ts'
import type { PublisherLogger, SpawnOutcome, Spawner } from '../src/publisher.ts'
import { buildRecord } from '../src/record.ts'
import type { FeedRecord, FeedState } from '../src/types.ts'
import { NOW, digest, meta } from './fixtures.ts'

interface Call {
  argv: readonly string[]
  record: FeedRecord
}

/** A fake CLI: records calls, answers from a queue (default exit 0), can hang until released. */
function fakeSpawner(): {
  spawn: Spawner
  calls: Call[]
  outcomes: Array<SpawnOutcome | Error>
  hold: boolean
  release(): void
} {
  const calls: Call[] = []
  const outcomes: Array<SpawnOutcome | Error> = []
  let waiters: Array<() => void> = []
  const api = {
    calls,
    outcomes,
    hold: false,
    release() {
      const pending = waiters
      waiters = []
      for (const wake of pending) wake()
    },
    spawn: async (argv: readonly string[], stdin: string): Promise<SpawnOutcome> => {
      calls.push({ argv, record: JSON.parse(stdin) as FeedRecord })
      if (api.hold) await new Promise<void>((resolve) => { waiters.push(resolve) })
      const next = outcomes.shift() ?? { exitCode: 0, stderr: '' }
      if (next instanceof Error) throw next
      return next
    },
  }
  return api
}

function fakeLogger(): PublisherLogger & { lines: string[] } {
  const lines: string[] = []
  return {
    lines,
    info: (message) => { lines.push(`info ${message}`) },
    warn: (message) => { lines.push(`warn ${message}`) },
  }
}

function record(state: FeedState, sessionId = 's_8f2a', signalAt: number | null = null): FeedRecord {
  return buildRecord({ meta: meta({ sessionId, signalAt }), digest: digest(), row: undefined, state, openUrl: null })
}

describe('Publisher', () => {
  beforeEach(() => { vi.useFakeTimers({ now: NOW }) })
  afterEach(() => { vi.useRealTimers() })

  it('runs `<binary> ai upsert --stdin` with the record as stdin after the debounce', async () => {
    const cli = fakeSpawner()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: '/opt/bin/todo', debounceMs: 300, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('unread'))
    expect(cli.calls).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(299)
    expect(cli.calls).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(1)
    expect(cli.calls).toHaveLength(1)
    expect(cli.calls[0]?.argv).toEqual(['/opt/bin/todo', ...UPSERT_ARGS])
    expect(cli.calls[0]?.record.state).toBe('unread')
    expect(cli.calls[0]?.record['ai-feed-spec']).toBe(1)
    expect(publisher.stats.published).toBe(1)
  })

  it('coalesces a burst for one session to the latest record', async () => {
    const cli = fakeSpawner()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 300, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('running'))
    publisher.schedule(record('unread'))
    publisher.schedule(record('seen'))
    await vi.advanceTimersByTimeAsync(300)
    expect(cli.calls.map(call => call.record.state)).toEqual(['seen'])
  })

  it('debounces per session, not globally', async () => {
    const cli = fakeSpawner()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 300, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('unread', 'a'))
    publisher.schedule(record('failed', 'b'))
    await vi.advanceTimersByTimeAsync(300)
    expect(cli.calls.map(call => [call.record.session_id, call.record.state])).toEqual([['a', 'unread'], ['b', 'failed']])
  })

  it('serializes deliveries per session so an older record cannot overtake a newer one', async () => {
    const cli = fakeSpawner()
    cli.hold = true
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 100, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('running'))
    await vi.advanceTimersByTimeAsync(100)
    expect(cli.calls).toHaveLength(1)
    publisher.schedule(record('unread'))
    await vi.advanceTimersByTimeAsync(100)
    // Second run waits behind the first.
    expect(cli.calls).toHaveLength(1)
    cli.release()
    await vi.advanceTimersByTimeAsync(0)
    expect(cli.calls).toHaveLength(2)
    cli.release()
    await publisher.flush()
    expect(cli.calls.map(call => call.record.state)).toEqual(['running', 'unread'])
  })

  it('does not re-send content that equals the last published record', async () => {
    const cli = fakeSpawner()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 10, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('unread'))
    await vi.advanceTimersByTimeAsync(10)
    publisher.schedule(record('unread', 's_8f2a', NOW + 5_000)) // same content, later event_at
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(1)
    expect(publisher.stats.unchanged).toBe(1)
    publisher.schedule(record('seen'))
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(2)
  })

  it('logs a non-zero exit and a spawn error, counting consecutive failures', async () => {
    const cli = fakeSpawner()
    const logger = fakeLogger()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 10, maxConsecutiveFailures: 5, logger })
    cli.outcomes.push({ exitCode: 2, stderr: 'error: unknown state "x"\n' })
    publisher.schedule(record('unread'))
    await vi.advanceTimersByTimeAsync(10)
    cli.outcomes.push(Object.assign(new Error('spawn todo ENOENT'), { code: 'ENOENT' }))
    publisher.schedule(record('seen'))
    await vi.advanceTimersByTimeAsync(10)
    expect(publisher.stats.failed).toBe(2)
    expect(publisher.stats.consecutiveFailures).toBe(2)
    expect(publisher.stats.paused).toBe(false)
    expect(logger.lines[0]).toMatch(/^warn todo-bridge: publish for session s_8f2a failed: todo ai upsert exited 2: error: unknown state "x"$/u)
    expect(logger.lines[1]).toMatch(/could not start todo: spawn todo ENOENT/u)
  })

  it('pauses after maxConsecutiveFailures, warns once, probes later, and resumes on success', async () => {
    const cli = fakeSpawner()
    const logger = fakeLogger()
    const publisher = new Publisher({
      spawn: cli.spawn, todoBinary: 'todo', debounceMs: 10, maxConsecutiveFailures: 3, logger, pauseProbeMs: 60_000,
    })
    for (let index = 0; index < 3; index += 1) {
      cli.outcomes.push({ exitCode: 1, stderr: '' })
      publisher.schedule(record('unread', `s${String(index)}`))
      await vi.advanceTimersByTimeAsync(10)
    }
    expect(publisher.stats.paused).toBe(true)
    expect(logger.lines.filter(line => line.includes('publishing paused'))).toHaveLength(1)
    expect(cli.calls).toHaveLength(3)

    // Inside the probe window: dropped without running the CLI.
    publisher.schedule(record('unread', 's9'))
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(3)
    expect(publisher.stats.dropped).toBe(1)

    // After the window: one probe; a failing probe keeps the pause without a new warn.
    await vi.advanceTimersByTimeAsync(60_000)
    cli.outcomes.push({ exitCode: 1, stderr: '' })
    publisher.schedule(record('unread', 's10'))
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(4)
    expect(publisher.stats.paused).toBe(true)
    expect(logger.lines.filter(line => line.startsWith('warn'))).toHaveLength(4)

    // Next window: the probe succeeds, publishing resumes, the counter resets.
    await vi.advanceTimersByTimeAsync(60_000)
    publisher.schedule(record('unread', 's11'))
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(5)
    expect(publisher.stats.paused).toBe(false)
    expect(publisher.stats.consecutiveFailures).toBe(0)
    expect(logger.lines.at(-1)).toMatch(/^info todo-bridge: todo is reachable again; publishing resumed$/u)
    publisher.schedule(record('unread', 's12'))
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(6)
  })

  it('a success resets the consecutive-failure counter before the threshold', async () => {
    const cli = fakeSpawner()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 10, maxConsecutiveFailures: 3, logger: fakeLogger() })
    cli.outcomes.push({ exitCode: 1, stderr: '' }, { exitCode: 1, stderr: '' }, { exitCode: 0, stderr: '' }, { exitCode: 1, stderr: '' })
    for (const id of ['a', 'b', 'c', 'd']) {
      publisher.schedule(record('unread', id))
      await vi.advanceTimersByTimeAsync(10)
    }
    expect(publisher.stats.paused).toBe(false)
    expect(publisher.stats.consecutiveFailures).toBe(1)
  })

  it('flush delivers pending records immediately', async () => {
    const cli = fakeSpawner()
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 10_000, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('unread', 'a'))
    publisher.schedule(record('unread', 'b'))
    await publisher.flush()
    expect(cli.calls).toHaveLength(2)
  })

  it('dispose clears timers, drops pending records, waits for in-flight runs, and rejects later records', async () => {
    const cli = fakeSpawner()
    cli.hold = true
    const publisher = new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 10, maxConsecutiveFailures: 5, logger: fakeLogger() })
    publisher.schedule(record('unread', 'a'))
    await vi.advanceTimersByTimeAsync(10)
    expect(cli.calls).toHaveLength(1)
    publisher.schedule(record('unread', 'b'))
    let settled = false
    const disposing = publisher.dispose().then(() => { settled = true })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(settled).toBe(false)
    cli.release()
    await disposing
    expect(settled).toBe(true)
    expect(cli.calls).toHaveLength(1)
    publisher.schedule(record('unread', 'c'))
    await vi.advanceTimersByTimeAsync(1_000)
    expect(cli.calls).toHaveLength(1)
  })

  it('rejects invalid policy values', () => {
    const cli = fakeSpawner()
    expect(() => new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: -1, maxConsecutiveFailures: 5, logger: fakeLogger() })).toThrow(/debounceMs/u)
    expect(() => new Publisher({ spawn: cli.spawn, todoBinary: 'todo', debounceMs: 0, maxConsecutiveFailures: 0, logger: fakeLogger() })).toThrow(/maxConsecutiveFailures/u)
  })
})
