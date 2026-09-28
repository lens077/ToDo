/**
 * Anonymous interface usage, sent to the self-hosted Umami at umami.apikv.com.
 *
 * What leaves the machine: the view name (`/goal`, `/reading` …), a handful
 * of action names, the app version, the platform, screen size and language.
 * Never task text, file paths, project names or anything from TODO.md.
 * Umami sets no cookie and keeps no user id; it derives a rotating session
 * hash from IP + user agent on the server.
 *
 * Off in dev builds and under automation (`navigator.webdriver`, which
 * Playwright sets), and whenever 设置 → 使用统计 is switched off. A failed
 * send is dropped: statistics must never slow down or break the board.
 */
import { currentAppSettings } from './appSettings';

const ENDPOINT = 'https://umami.apikv.com/api/send';
/** Umami website 「Virgo 桌面端 (ToDo)」. Public by design, like any tracker id. */
const WEBSITE_ID = 'd6695c23-e61a-4901-8771-59eb2d81afde';

type EventData = Record<string, string | number | boolean>;

let currentPath = '/';

function platform(): string {
  if (typeof window === 'undefined') return 'unknown';
  const shell = '__TAURI_INTERNALS__' in window ? 'desktop' : 'web';
  const ua = navigator.userAgent;
  const os = /Mac/.test(ua) ? 'macos' : /Win/.test(ua) ? 'windows' : /Linux/.test(ua) ? 'linux' : 'other';
  return `${shell}-${os}`;
}

function enabled(): boolean {
  if (import.meta.env.DEV) return false;
  if (typeof navigator === 'undefined' || navigator.webdriver) return false;
  return currentAppSettings().usageAnalytics;
}

function send(payload: Record<string, unknown>) {
  if (!enabled()) return;
  const body = JSON.stringify({
    type: 'event',
    payload: {
      website: WEBSITE_ID,
      hostname: platform(),
      language: navigator.language,
      screen: `${window.screen.width}x${window.screen.height}`,
      tag: __APP_VERSION__,
      ...payload,
    },
  });
  fetch(ENDPOINT, { method: 'POST', keepalive: true, headers: { 'Content-Type': 'application/json' }, body }).catch(() => {
    // Offline or blocked: drop it.
  });
}

/** A view the person switched to. Consecutive views form the path Umami's Journey report draws. */
export function trackView(path: string, title?: string) {
  if (path === currentPath && title === undefined) return;
  currentPath = path;
  send({ url: path, title: title ?? path });
}

/** A named action inside the current view. `data` must never carry user content. */
export function trackEvent(name: string, data?: EventData) {
  send({ url: currentPath, name, ...(data ? { data } : {}) });
}
