import * as React from 'react';
import { authBridge, type AuthStatus } from '../lib/auth';

export type AuthBusy = 'login' | 'refresh' | 'unbind' | null;

/**
 * 设置页的登录态。状态来自 Rust（`auth_status`），登录/解绑的结果通过
 * `auth://changed` 事件补上；浏览器 mock 直接返回伪造状态，供 e2e 使用。
 */
export function useAuth() {
  const [status, setStatus] = React.useState<AuthStatus | null>(null);
  const [busy, setBusy] = React.useState<AuthBusy>(null);
  const [error, setError] = React.useState<string | null>(null);

  // Rust 的 `status.error` 也要冒到界面上：激活失败可能发生在登录回跳的
  // 后台线程里，前端只收到事件，不会收到被 reject 的 promise。
  const applyStatus = React.useCallback((next: AuthStatus) => {
    setStatus(next);
    setError(next.error ?? null);
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    void authBridge
      .status()
      .then((next) => {
        if (!cancelled) applyStatus(next);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(String(cause));
      });
    let unlisten: (() => void) | null = null;
    void authBridge
      .subscribe((next) => applyStatus(next))
      .then((stop) => {
        unlisten = stop;
        if (cancelled) stop();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [applyStatus]);

  const run = React.useCallback(
    async (action: Exclude<AuthBusy, null>, command: () => Promise<AuthStatus>) => {
      setBusy(action);
      setError(null);
      try {
        const next = await command();
        applyStatus(next);
        return next;
      } catch (cause) {
        setError(String(cause));
        return null;
      } finally {
        setBusy(null);
      }
    },
    [applyStatus],
  );

  const login = React.useCallback(() => run('login', () => authBridge.login()), [run]);
  const refresh = React.useCallback(() => run('refresh', () => authBridge.refresh()), [run]);
  const unbind = React.useCallback(() => run('unbind', () => authBridge.unbind()), [run]);

  return { status, busy, error, login, refresh, unbind };
}
