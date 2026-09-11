/**
 * The client half against a fake `ctx.sessions`: the deep link opens the
 * session when the list already has it, when it arrives later, and gives up
 * with a warning otherwise; the parameter is stripped only after a
 * successful open.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DEEP_LINK_TIMEOUT_MS,
  apply,
  inject,
  listHasSession,
  runDeepLink,
  sessionIdOf,
  withoutSessionParam,
  type DeepLinkContext,
  type DeepLinkEnvironment,
} from '../src/client/index.ts'
import type { SessionListState } from '@deepseek-ai/dsh-api-session-controller/client'

const SESSION = 'session-69ebcc9b-08b3-4ec3-8e44-fd2b8259fe18'

interface FakeClient {
  ctx: DeepLinkContext
  env: DeepLinkEnvironment
  href: string
  replaced: string[]
  opened: string[]
  refreshes: number
  warnings: string[]
  infos: string[]
  /** Replace the list rows and notify subscribers, as the store does. */
  setRows(ids: string[], phase?: 'pending' | 'ready'): void
  openThrows: boolean
  refreshRejects: boolean
}

function listState(ids: string[], phase: 'pending' | 'ready' = 'ready'): SessionListState {
  const byId: Record<string, unknown> = {}
  for (const id of ids) byId[id] = { sessionId: id }
  return {
    ids,
    byId,
    current: undefined,
    phase,
    subagentsByParent: {},
    jobsBySession: {},
    currentAddress: undefined,
  } as unknown as SessionListState
}

function makeClient(href: string, ids: string[] = [], phase: 'pending' | 'ready' = 'ready'): FakeClient {
  let state = listState(ids, phase)
  const subscribers = new Set<() => void>()
  const client: FakeClient = {
    href,
    replaced: [],
    opened: [],
    refreshes: 0,
    warnings: [],
    infos: [],
    openThrows: false,
    refreshRejects: false,
    setRows(next, nextPhase = 'ready') {
      state = listState(next, nextPhase)
      for (const fn of subscribers) fn()
    },
    ctx: {
      sessions: {
        list: {
          getSnapshot: () => state,
          subscribe: (fn) => {
            subscribers.add(fn)
            return () => { subscribers.delete(fn) }
          },
        },
        open: (id) => {
          if (client.openThrows) throw new Error(`sessions.select: unknown session ${id}`)
          client.opened.push(id)
        },
        refresh: async () => {
          client.refreshes += 1
          if (client.refreshRejects) throw new Error('offline')
        },
      },
      logger: {
        warn: (message) => { client.warnings.push(message) },
        info: (message) => { client.infos.push(message) },
      },
    },
    env: {
      href: () => client.href,
      replaceUrl: (url) => {
        client.replaced.push(url)
        client.href = url
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (handle) => { clearTimeout(handle as ReturnType<typeof setTimeout>) },
    },
  }
  return client
}

describe('deep link URL helpers', () => {
  it('reads a non-empty session parameter', () => {
    expect(sessionIdOf(`http://127.0.0.1:3080/?session=${SESSION}`)).toBe(SESSION)
    expect(sessionIdOf('http://127.0.0.1:3080/?session=')).toBeNull()
    expect(sessionIdOf('http://127.0.0.1:3080/')).toBeNull()
    expect(sessionIdOf('http://127.0.0.1:3080/?other=1#frag')).toBeNull()
    expect(sessionIdOf('not a url')).toBeNull()
    expect(sessionIdOf('http://127.0.0.1:3080/?session=session-1%2F2')).toBe('session-1/2')
  })

  it('strips only the session parameter', () => {
    expect(withoutSessionParam(`http://127.0.0.1:3080/?session=${SESSION}`)).toBe('http://127.0.0.1:3080/')
    expect(withoutSessionParam(`http://127.0.0.1:3080/?a=1&session=${SESSION}&b=2#x`)).toBe('http://127.0.0.1:3080/?a=1&b=2#x')
    expect(withoutSessionParam('http://127.0.0.1:3080/')).toBe('http://127.0.0.1:3080/')
  })

  it('finds a session in the list rows or the addressed byId map', () => {
    expect(listHasSession(listState([SESSION]), SESSION)).toBe(true)
    expect(listHasSession(listState([]), SESSION)).toBe(false)
    const addressed = listState([])
    ;(addressed.byId as Record<string, unknown>)[SESSION] = { sessionId: SESSION }
    expect(listHasSession(addressed, SESSION)).toBe(true)
    expect(listHasSession(listState([SESSION]), 'constructor')).toBe(false)
  })

  it('declares the sessions service as its only inject edge', () => {
    expect(inject).toEqual(['sessions'])
  })
})

describe('runDeepLink', () => {
  beforeEach(() => { vi.useFakeTimers() })
  afterEach(() => { vi.useRealTimers() })

  it('does nothing without a session parameter', async () => {
    const client = makeClient('http://127.0.0.1:3080/', [SESSION])
    runDeepLink(client.ctx, client.env)
    await vi.advanceTimersByTimeAsync(DEEP_LINK_TIMEOUT_MS + 1)
    expect(client.opened).toEqual([])
    expect(client.refreshes).toBe(0)
    expect(client.replaced).toEqual([])
    expect(client.warnings).toEqual([])
  })

  it('opens immediately when the list already carries the id, then strips the parameter', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, ['session-other', SESSION])
    runDeepLink(client.ctx, client.env)
    expect(client.opened).toEqual([SESSION])
    expect(client.replaced).toEqual(['http://127.0.0.1:3080/'])
    expect(client.href).toBe('http://127.0.0.1:3080/')
    expect(client.refreshes).toBe(0)
    await vi.advanceTimersByTimeAsync(DEEP_LINK_TIMEOUT_MS + 1)
    expect(client.opened).toEqual([SESSION])
    expect(client.warnings).toEqual([])
    expect(client.infos.some(line => line.includes(SESSION))).toBe(true)
  })

  it('waits for a later list update, refreshing a settled list once meanwhile', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}&theme=dark`, ['session-other'])
    runDeepLink(client.ctx, client.env)
    expect(client.opened).toEqual([])
    await vi.advanceTimersByTimeAsync(0)
    expect(client.refreshes).toBe(1)
    await vi.advanceTimersByTimeAsync(2_000)
    client.setRows(['session-other', 'session-unrelated'])
    expect(client.opened).toEqual([])
    await vi.advanceTimersByTimeAsync(0)
    expect(client.refreshes).toBe(1)
    client.setRows(['session-other', SESSION])
    expect(client.opened).toEqual([SESSION])
    expect(client.replaced).toEqual(['http://127.0.0.1:3080/?theme=dark'])
    // Later updates and the timer no longer act.
    client.setRows([SESSION])
    await vi.advanceTimersByTimeAsync(DEEP_LINK_TIMEOUT_MS + 1)
    expect(client.opened).toEqual([SESSION])
    expect(client.warnings).toEqual([])
  })

  it('does not refresh while the first pull is pending; refreshes once it lands without the id', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [], 'pending')
    runDeepLink(client.ctx, client.env)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(client.refreshes).toBe(0)
    client.setRows(['session-other'], 'ready')
    await vi.advanceTimersByTimeAsync(0)
    expect(client.refreshes).toBe(1)
    client.setRows(['session-other', SESSION], 'ready')
    expect(client.opened).toEqual([SESSION])
    expect(client.replaced).toEqual(['http://127.0.0.1:3080/'])
  })

  it('opens from the first pull without refreshing when the id is in it', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [], 'pending')
    runDeepLink(client.ctx, client.env)
    client.setRows([SESSION], 'ready')
    await vi.advanceTimersByTimeAsync(0)
    expect(client.opened).toEqual([SESSION])
    expect(client.refreshes).toBe(0)
  })

  it('gives up with a warning after the timeout and leaves the URL alone', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [])
    runDeepLink(client.ctx, client.env)
    await vi.advanceTimersByTimeAsync(DEEP_LINK_TIMEOUT_MS - 1)
    expect(client.warnings).toEqual([])
    await vi.advanceTimersByTimeAsync(2)
    expect(client.warnings).toHaveLength(1)
    expect(client.warnings[0]).toContain(SESSION)
    expect(client.warnings[0]).toContain('15000 ms')
    expect(client.opened).toEqual([])
    expect(client.replaced).toEqual([])
    // A row arriving after the give-up is ignored.
    client.setRows([SESSION])
    expect(client.opened).toEqual([])
  })

  it('honours a custom timeout and a failing refresh', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [])
    client.refreshRejects = true
    runDeepLink(client.ctx, client.env, { timeoutMs: 500 })
    await vi.advanceTimersByTimeAsync(0)
    expect(client.warnings.some(line => line.includes('refresh') && line.includes('offline'))).toBe(true)
    await vi.advanceTimersByTimeAsync(501)
    expect(client.warnings.some(line => line.includes('500 ms'))).toBe(true)
  })

  it('warns instead of throwing when open() rejects the id, and keeps the parameter', () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [SESSION])
    client.openThrows = true
    expect(() => runDeepLink(client.ctx, client.env)).not.toThrow()
    expect(client.warnings).toHaveLength(1)
    expect(client.warnings[0]).toContain('unknown session')
    expect(client.replaced).toEqual([])
  })

  it('the disposer cancels the wait', async () => {
    const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [])
    const dispose = runDeepLink(client.ctx, client.env)
    dispose()
    client.setRows([SESSION])
    await vi.advanceTimersByTimeAsync(DEEP_LINK_TIMEOUT_MS + 1)
    expect(client.opened).toEqual([])
    expect(client.warnings).toEqual([])
  })
})

describe('apply', () => {
  it('is a no-op outside a browser (no location / history globals)', () => {
    expect(typeof location).toBe('undefined')
    const effects: string[] = []
    const ctx = {
      sessions: undefined,
      logger: { warn: () => {}, info: () => {} },
      effect: (_fn: unknown, label: string) => { effects.push(label) },
    }
    expect(() => { apply(ctx as never) }).not.toThrow()
    expect(effects).toEqual([])
  })

  it('registers one effect that disposes the wait in a browser-like global scope', () => {
    const globals = globalThis as { location?: unknown; history?: unknown }
    globals.location = { href: `http://127.0.0.1:3080/?session=${SESSION}` }
    globals.history = { state: null, replaceState: () => {} }
    try {
      const client = makeClient(`http://127.0.0.1:3080/?session=${SESSION}`, [])
      let disposer: (() => void) | undefined
      const ctx = {
        ...client.ctx,
        effect: (fn: () => () => void, _label: string) => { disposer = fn() },
      }
      apply(ctx as never)
      expect(typeof disposer).toBe('function')
      disposer?.()
      client.setRows([SESSION])
      expect(client.opened).toEqual([])
    } finally {
      delete globals.location
      delete globals.history
    }
  })
})
