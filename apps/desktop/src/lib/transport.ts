/**
 * Where the UI gets its data.
 *
 * The same React app runs in three places: the Tauri desktop shell, a browser
 * against `todo serve`, and a browser with no backend at all (the mock used by
 * the e2e suite). Choosing between them here — rather than at each call site —
 * is what keeps a second, drifting copy of the UI from appearing.
 */
export type TransportKind = 'tauri' | 'http' | 'mock';

/** Server capabilities that change what the UI may offer. */
export interface TransportCapabilities {
  /** False when the server refuses writes, e.g. a phone pointed at --read-only. */
  writable: boolean;
  project: string | null;
}

function hasTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Base URL of the HTTP API, when one is reachable.
 *
 * A build served by `todo serve --web-root` is same-origin, so the default needs
 * no configuration. `?api=` overrides it for development against a separate
 * Vite dev server.
 */
export function apiBase(): string {
  if (typeof window === 'undefined') return '';
  const override = new URLSearchParams(window.location.search).get('api');
  if (override) return override.replace(/\/$/, '');
  return `${window.location.origin}/api/v1`;
}

export function detectTransport(): TransportKind {
  if (hasTauri()) return 'tauri';
  // A page served from a real origin may have `todo serve` behind it; probing
  // happens in `probeHttp`, which the caller awaits before committing.
  if (typeof window !== 'undefined' && window.location.protocol.startsWith('http')) {
    return 'http';
  }
  return 'mock';
}

/**
 * Confirms an HTTP backend is actually there and reports what it allows.
 *
 * Returns null when nothing answers, which is the signal to fall back to mock
 * data rather than showing an app that fails on every interaction.
 */
export async function probeHttp(signal?: AbortSignal): Promise<TransportCapabilities | null> {
  try {
    const response = await fetch(`${apiBase()}/health`, { signal });
    if (!response.ok) return null;
    const body = (await response.json()) as {
      ok?: boolean;
      read_only?: boolean;
      project?: string;
    };
    if (!body.ok) return null;
    return {
      writable: body.read_only !== true,
      project: body.project ?? null,
    };
  } catch {
    return null;
  }
}

export class HttpError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = 'HttpError';
  }
}

/** Issues a request and unwraps the server's error envelope. */
export async function api<T>(
  path: string,
  init?: { method?: string; body?: unknown },
): Promise<T> {
  const response = await fetch(`${apiBase()}${path}`, {
    method: init?.method ?? 'GET',
    headers: init?.body ? { 'Content-Type': 'application/json' } : undefined,
    body: init?.body ? JSON.stringify(init.body) : undefined,
  });
  const text = await response.text();
  const parsed: unknown = text ? JSON.parse(text) : null;
  if (!response.ok) {
    const message =
      parsed && typeof parsed === 'object' && 'error' in parsed
        ? String((parsed as { error: unknown }).error)
        : `请求失败（HTTP ${response.status}）`;
    throw new HttpError(message, response.status);
  }
  return parsed as T;
}
