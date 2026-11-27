// Pro 图片代理的账号桥：登录、刷新、解绑。令牌不经过这里——它只在 Rust
// 内存里，抓图命令直接取用；前端只看到登录态。

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface AuthStatus {
  signed_in: boolean;
  pro: boolean;
  sub: string | null;
  device_name: string | null;
  /** unix 秒 */
  bound_at: number | null;
  /** unix 秒 */
  token_expires_at: number | null;
  /** 浏览器登录进行中 */
  pending: boolean;
  proxy_base: string;
  /** 当前有可用令牌：抓图才会真的走代理 */
  proxy_enabled: boolean;
  error: string | null;
}

export const AUTH_EVENT = 'auth://changed';

const signedOut: AuthStatus = {
  signed_in: false,
  pro: false,
  sub: null,
  device_name: null,
  bound_at: null,
  token_expires_at: null,
  pending: false,
  proxy_base: '',
  proxy_enabled: false,
  error: null,
};

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Browser mock: tests set `window.__todoMockAuth` to start signed in. */
declare global {
  interface Window {
    __todoMockAuth?: Partial<AuthStatus> | null;
  }
}

function mockStatus(): AuthStatus {
  return { ...signedOut, proxy_base: 'https://virgo-img.apikv.com/img?url=', ...(window.__todoMockAuth ?? {}) };
}

async function call(command: string): Promise<AuthStatus> {
  return invoke<AuthStatus>(command);
}

export const authBridge = {
  async status(): Promise<AuthStatus> {
    if (isTauriRuntime()) return call('auth_status');
    return mockStatus();
  },

  /** Opens the browser and returns immediately; the result arrives as an event. */
  async login(): Promise<AuthStatus> {
    if (isTauriRuntime()) return call('auth_start_login');
    window.__todoMockAuth = {
      signed_in: true,
      pro: true,
      sub: 'mock-user',
      device_name: 'Mockbook · macOS',
      bound_at: Date.now() / 1000 - 3600,
      token_expires_at: Date.now() / 1000 + 12 * 3600,
      pending: false,
      proxy_enabled: true,
    };
    return mockStatus();
  },

  async refresh(): Promise<AuthStatus> {
    if (isTauriRuntime()) return call('auth_refresh');
    return mockStatus();
  },

  async unbind(): Promise<AuthStatus> {
    if (isTauriRuntime()) return call('auth_unbind');
    window.__todoMockAuth = null;
    return mockStatus();
  },

  /** Subscribes to login/logout changes; resolves to an unlistener. */
  async subscribe(listener: (status: AuthStatus) => void): Promise<UnlistenFn> {
    if (!isTauriRuntime()) return () => {};
    return listen<AuthStatus>(AUTH_EVENT, (event) => listener(event.payload));
  },
};
