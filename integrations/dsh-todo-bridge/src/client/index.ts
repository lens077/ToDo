/**
 * dsh-todo-bridge client half: a one-shot `?session=<id>` deep-link handler
 * for the DSH Web GUI. ToDo's 「打开会话」 opens the bridge's `open_url`
 * (`{base}/?session={sessionId}` by default); this plugin selects that
 * session once the client session list carries it, then strips the query
 * parameter so a plain reload does not jump again.
 *
 * The Web shell has no client-side router: a new URL in the same tab is a
 * full document load, so running once at boot covers ToDo's window reuse as
 * well as a fresh tab. `history.replaceState` fires no `popstate`.
 *
 * The bundle is a factory-form CJS artifact (`lib/client.js`) registered
 * through `window.__ModuleLoader__.load({ id: 'dsh-todo-bridge', factory })`;
 * the host serves it because `package.json` declares `dsh.client`.
 *
 * @module dsh-todo-bridge/client
 */

import type { Context } from '@deepseek-ai/cordis'
import type { ISessions, SessionListState } from '@deepseek-ai/dsh-api-session-controller/client'
import type { SessionId } from '@deepseek-ai/dsh-session/types'

/** Query parameter carrying the target session id. */
export const SESSION_PARAM = 'session'

/** How long the handler waits for the session list to carry the id. */
export const DEEP_LINK_TIMEOUT_MS = 15_000

/** Cordis client services the plugin reads (`ctx.sessions` from the session controller's client half). */
export const inject = ['sessions']

/** The slice of `ctx.sessions` the handler uses. */
export type DeepLinkSessions = Pick<ISessions, 'list' | 'open' | 'refresh'>

/** The slice of the client context the handler uses. */
export interface DeepLinkContext {
  readonly sessions: DeepLinkSessions
  readonly logger: { warn(message: string): void; info(message: string): void }
}

/** Browser seams, replaceable in tests. */
export interface DeepLinkEnvironment {
  /** Current document URL. */
  readonly href: () => string
  /** Replace the current history entry's URL without navigating. */
  readonly replaceUrl: (url: string) => void
  readonly setTimeout: (fn: () => void, ms: number) => unknown
  readonly clearTimeout: (handle: unknown) => void
}

/** Handler options. */
export interface DeepLinkOptions {
  /** Give-up delay in milliseconds. Omit for {@link DEEP_LINK_TIMEOUT_MS}. */
  readonly timeoutMs?: number
}

/**
 * Read the deep-link target from a URL.
 * @param href - absolute document URL.
 * @returns the non-empty session id, or `null` when the URL carries none.
 */
export function sessionIdOf(href: string): string | null {
  let url: URL
  try {
    url = new URL(href)
  } catch {
    return null
  }
  const value = url.searchParams.get(SESSION_PARAM)
  return value === null || value.length === 0 ? null : value
}

/**
 * The same URL without the `session` parameter (other parameters and the
 * hash are kept; an emptied query drops its `?`).
 * @param href - absolute document URL.
 * @returns the URL to leave in the address bar.
 */
export function withoutSessionParam(href: string): string {
  const url = new URL(href)
  url.searchParams.delete(SESSION_PARAM)
  return url.toString()
}

/**
 * Whether the client list carries a session the controller can `open()`.
 * `byId` also holds the addressed subagent route, which `open()` accepts.
 * @param state - list snapshot.
 * @param id - session id from the URL.
 * @returns true when `open(id)` will not fail for an unknown id.
 */
export function listHasSession(state: SessionListState, id: string): boolean {
  return Object.hasOwn(state.byId, id) || state.ids.includes(id as SessionId)
}

/**
 * Run the deep link once: open the session as soon as the list carries it,
 * then strip the parameter. Gives up with a warning after the timeout.
 * @param ctx - client context slice.
 * @param env - browser seams.
 * @param options - timeout override.
 * @returns a disposer that cancels the wait.
 */
export function runDeepLink(ctx: DeepLinkContext, env: DeepLinkEnvironment, options: DeepLinkOptions = {}): () => void {
  const id = sessionIdOf(env.href())
  if (id === null) return () => {}
  const timeoutMs = options.timeoutMs ?? DEEP_LINK_TIMEOUT_MS
  let done = false
  let unsubscribe: (() => void) | undefined
  let timer: unknown

  const finish = (): void => {
    done = true
    unsubscribe?.()
    unsubscribe = undefined
    if (timer !== undefined) env.clearTimeout(timer)
    timer = undefined
  }

  const open = (): void => {
    finish()
    try {
      ctx.sessions.open(id as SessionId)
    } catch (error) {
      ctx.logger.warn(`todo-bridge: opening session "${id}" from the deep link failed: ${String(error)}`)
      return
    }
    ctx.logger.info(`todo-bridge: opened session "${id}" from the deep link`)
    try {
      env.replaceUrl(withoutSessionParam(env.href()))
    } catch (error) {
      ctx.logger.warn(`todo-bridge: stripping ?${SESSION_PARAM} from the URL failed: ${String(error)}`)
    }
  }

  // One extra pull at most: only once a pull has landed (`phase: 'ready'`)
  // without the id. Before that the shell's own connect-time pull is on its
  // way, and asking earlier would only fail against a transport that is not
  // connected yet.
  let refreshed = false
  const refreshOnce = (): void => {
    if (refreshed) return
    refreshed = true
    void Promise.resolve()
      .then(() => ctx.sessions.refresh())
      .catch((error: unknown) => {
        ctx.logger.warn(`todo-bridge: session list refresh for the deep link failed: ${String(error)}`)
      })
  }

  const check = (): boolean => {
    if (done) return true
    const state = ctx.sessions.list.getSnapshot()
    if (listHasSession(state, id)) {
      open()
      return true
    }
    if (state.phase === 'ready') refreshOnce()
    return false
  }

  if (check()) return finish

  unsubscribe = ctx.sessions.list.subscribe(() => { check() })
  timer = env.setTimeout(() => {
    if (done) return
    finish()
    ctx.logger.warn(
      `todo-bridge: session "${id}" did not appear in the session list within ${String(timeoutMs)} ms; deep link ignored`,
    )
  }, timeoutMs)
  return finish
}

/** Browser seams over `location`, `history`, and the global timers. */
export function browserEnvironment(): DeepLinkEnvironment | undefined {
  if (typeof location === 'undefined' || typeof history === 'undefined') return undefined
  return {
    href: () => location.href,
    replaceUrl: (url) => { history.replaceState(history.state, '', url) },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => { clearTimeout(handle as ReturnType<typeof setTimeout>) },
  }
}

/**
 * Mount the deep-link handler on the client tree.
 * @param ctx - client Cordis context with `ctx.sessions` injected.
 */
export function apply(ctx: Context): void {
  const env = browserEnvironment()
  if (env === undefined) return
  const dispose = runDeepLink(ctx as unknown as DeepLinkContext, env)
  ctx.effect(() => dispose, 'todo-bridge: session deep link')
}
