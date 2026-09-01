import * as React from 'react';
import {
  compareRevisions,
  desktop,
  DesktopBridgeError,
  type Capture,
  type DesktopMutation,
  type DesktopSnapshot,
  type EvidenceRange,
  type TriageResult,
} from '../lib/desktop';

export type LoadPhase = 'loading' | 'ready' | 'no-project' | 'load-error';

export interface AppBanner {
  tone: 'warning' | 'error' | 'info';
  title: string;
  detail: string;
}

export interface PartialTriageState {
  taskId: string;
  message: string;
}

export interface WorkspaceStatus {
  projectPath: string | null;
  syncState: 'watching' | 'idle' | 'error';
  shortcutState: 'registered' | 'unavailable' | 'unknown';
  shortcutMessage?: string;
  startupMs: number | null;
}

const fallbackStatus: WorkspaceStatus = {
  projectPath: null,
  syncState: 'idle',
  shortcutState: 'unknown',
  startupMs: null,
};

function bannerFor(error: unknown): AppBanner {
  const bridgeError =
    error instanceof DesktopBridgeError ? error : new DesktopBridgeError(error instanceof Error ? error.message : String(error));
  switch (bridgeError.kind) {
    case 'conflict':
      return {
        tone: 'warning',
        title: '文件已在外部更新，请重试',
        detail: '已放弃这次写入，并尝试重新载入 TODO.md 的最新状态。',
      };
    case 'locked':
      return {
        tone: 'warning',
        title: '项目文件暂时繁忙',
        detail: '等待文件锁超时，本次操作没有写入。稍后重试即可。',
      };
    case 'parse':
      return {
        tone: 'error',
        title: 'TODO.md 暂时无法解析',
        detail: '界面保持只读，修复 frontmatter 或 Git 冲突标记后再重新载入。',
      };
    case 'no_project':
      return {
        tone: 'info',
        title: '尚未打开项目',
        detail: '选择一个包含 TODO.md 的仓库后再继续。',
      };
    default:
      return {
        tone: 'error',
        title: '操作没有完成',
        detail: bridgeError.message || '桌面后端返回了未知错误，请重试。',
      };
  }
}

export function useDesktop() {
  const [phase, setPhase] = React.useState<LoadPhase>('loading');
  const [snapshot, setSnapshot] = React.useState<DesktopSnapshot | null>(null);
  const [status, setStatus] = React.useState<WorkspaceStatus>(fallbackStatus);
  const [captures, setCaptures] = React.useState<Capture[]>([]);
  const [capturesLoading, setCapturesLoading] = React.useState(true);
  const [banner, setBanner] = React.useState<AppBanner | null>(null);
  const [busyKeys, setBusyKeys] = React.useState<Set<string>>(() => new Set());
  const [partialTriages, setPartialTriages] = React.useState<Record<string, PartialTriageState>>({});
  const snapshotRef = React.useRef<DesktopSnapshot | null>(null);
  const staleRevisions = React.useRef(new Set<string>());

  const applySnapshot = React.useCallback((next: DesktopSnapshot, authoritative = false) => {
    const current = snapshotRef.current;
    if (current?.revision === next.revision || staleRevisions.current.has(next.revision)) return false;
    if (current) {
      const comparison = compareRevisions(next.revision, current.revision);
      if (comparison !== null && comparison < 0) return false;
      if (authoritative || comparison === null || comparison > 0) {
        staleRevisions.current.add(current.revision);
        if (staleRevisions.current.size > 64) {
          const oldest = staleRevisions.current.values().next().value;
          if (oldest) staleRevisions.current.delete(oldest);
        }
      }
    }
    snapshotRef.current = next;
    setSnapshot(next);
    setStatus((value) => ({
      ...value,
      projectPath: next.project_path,
      syncState: 'watching',
    }));
    setPhase('ready');
    return true;
  }, []);

  const loadCaptures = React.useCallback(async () => {
    setCapturesLoading(true);
    try {
      setCaptures(await desktop.captureList());
    } catch (error) {
      setBanner(bannerFor(error));
    } finally {
      setCapturesLoading(false);
    }
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;

    void desktop
      .subscribe((event) => {
        if (cancelled) return;
        if (event.type === 'snapshot') {
          applySnapshot(event.snapshot);
          setBanner(null);
        } else {
          setBanner(bannerFor(event.error));
          setStatus((value) => ({ ...value, syncState: 'error' }));
        }
      })
      .then((remove) => {
        if (cancelled) remove();
        else unsubscribe = remove;
      })
      .catch((error) => {
        if (!cancelled) setBanner(bannerFor(error));
      });

    void (async () => {
      try {
        const nextStatus = await desktop.status();
        if (cancelled) return;
        setStatus((value) => ({
          ...value,
          startupMs: nextStatus.startup_ms,
          shortcutState:
            nextStatus.shortcut_status.status === 'registered'
              ? 'registered'
              : nextStatus.shortcut_status.status === 'error'
                ? 'unavailable'
                : 'unknown',
          shortcutMessage:
            nextStatus.shortcut_status.status === 'error'
              ? nextStatus.shortcut_status.message
              : undefined,
        }));
      } catch (error) {
        if (cancelled) return;
        setBanner(bannerFor(error));
      }

      try {
        const initialSnapshot = await desktop.snapshot();
        if (!cancelled) applySnapshot(initialSnapshot, true);
      } catch (error) {
        if (cancelled) return;
        const bridgeError = error instanceof DesktopBridgeError ? error : new DesktopBridgeError(String(error));
        if (bridgeError.kind === 'no_project') {
          setPhase('no-project');
        } else {
          setPhase('load-error');
          setBanner(bannerFor(error));
        }
      }
    })();
    void loadCaptures();

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [applySnapshot, loadCaptures]);

  const withBusy = React.useCallback(async <T,>(key: string, operation: () => Promise<T>): Promise<T> => {
    setBusyKeys((current) => new Set(current).add(key));
    try {
      return await operation();
    } finally {
      setBusyKeys((current) => {
        const next = new Set(current);
        next.delete(key);
        return next;
      });
    }
  }, []);

  const refreshSnapshot = React.useCallback(async () => {
    try {
      const next = await desktop.snapshot();
      applySnapshot(next, true);
      setBanner(null);
    } catch (error) {
      setBanner(bannerFor(error));
      const kind = error instanceof DesktopBridgeError ? error.kind : 'unknown';
      if (kind === 'parse') setStatus((value) => ({ ...value, syncState: 'error' }));
    }
  }, [applySnapshot]);

  const runMutation = React.useCallback(
    async (key: string, intent: DesktopMutation): Promise<boolean> => {
      try {
        await withBusy(key, async () => {
          const authoritative = await desktop.mutate(intent);
          applySnapshot(authoritative, true);
        });
        setBanner(null);
        return true;
      } catch (error) {
        setBanner(bannerFor(error));
        const kind = error instanceof DesktopBridgeError ? error.kind : 'unknown';
        if (kind === 'conflict') void refreshSnapshot();
        return false;
      }
    },
    [applySnapshot, refreshSnapshot, withBusy],
  );

  const chooseProject = React.useCallback(async () => {
    try {
      const opened = await withBusy('project', () => desktop.chooseAndOpenProject());
      if (!opened) return;
      applySnapshot(opened, true);
      setBanner(null);
      setPhase('ready');
    } catch (error) {
      setBanner(bannerFor(error));
      if (!snapshotRef.current) setPhase('load-error');
    }
  }, [applySnapshot, withBusy]);

  const appendCapture = React.useCallback(
    async (text: string): Promise<boolean> => {
      try {
        const created = await withBusy('capture:new', () => desktop.captureAppend(text));
        setCaptures(created.captures);
        setBanner(null);
        return true;
      } catch (error) {
        setBanner(bannerFor(error));
        return false;
      }
    },
    [withBusy],
  );

  const deleteCapture = React.useCallback(
    async (id: string): Promise<boolean> => {
      try {
        const authoritative = await withBusy(`capture:${id}`, () => desktop.captureDelete(id));
        setCaptures(authoritative);
        setPartialTriages((current) => {
          const next = { ...current };
          delete next[id];
          return next;
        });
        setBanner(null);
        return true;
      } catch (error) {
        setBanner(bannerFor(error));
        return false;
      }
    },
    [withBusy],
  );

  const triageCapture = React.useCallback(
    async (id: string, column: string): Promise<TriageResult | null> => {
      try {
        const result = await withBusy(`capture:${id}`, () => desktop.captureTriage(id, column));
        if (result.snapshot) applySnapshot(result.snapshot, true);
        if (result.captures) {
          setCaptures(result.captures);
        } else if (result.status === 'completed') {
          // The tagged result guarantees deletion even when the follow-up list read is unavailable.
          setCaptures((current) => current.filter((capture) => capture.id !== id));
        }
        if (result.status === 'task_created_capture_retained') {
          setPartialTriages((current) => ({
            ...current,
            [id]: {
              taskId: result.task_id,
              message: result.message ?? '任务已创建，Capture 仍保留。',
            },
          }));
        } else {
          setPartialTriages((current) => {
            const next = { ...current };
            delete next[id];
            return next;
          });
        }
        const unavailable = [result.snapshot_error, result.captures_error].filter(Boolean);
        setBanner(
          unavailable.length
            ? {
                tone: 'warning',
                title: '任务已创建，列表刷新未完成',
                detail: unavailable.join('；'),
              }
            : null,
        );
        return result;
      } catch (error) {
        setBanner(bannerFor(error));
        return null;
      }
    },
    [applySnapshot, withBusy],
  );

  return {
    phase,
    snapshot,
    status,
    captures,
    capturesLoading,
    banner,
    busyKeys,
    partialTriages,
    chooseProject,
    refreshSnapshot,
    // Adopts a snapshot from a write that bypassed `mutate`, such as a confirmed
    // rewrite. Authoritative: the caller already wrote the file.
    applySnapshot: (next: DesktopSnapshot) => {
      applySnapshot(next, true);
    },
    clearBanner: () => setBanner(null),
    addTask: (body: string, column: string | null) =>
      runMutation('task:new', { type: 'add_task', body, column }),
    setCompleted: (id: string, completed: boolean, note?: string, evidence?: EvidenceRange) =>
      runMutation(`task:${id}`, { type: 'set_completed', id, completed, note, evidence }),
    setPartial: (id: string, percent: number, note: string) =>
      runMutation(`task:${id}`, { type: 'set_partial', id, percent, note }),
    setFailed: (id: string, note: string) =>
      runMutation(`task:${id}`, { type: 'set_failed', id, note }),
    moveTask: (id: string, column: string) =>
      runMutation(`task:${id}`, { type: 'move_task', id, column, target: { type: 'column_end' } }),
    appendCapture,
    deleteCapture,
    triageCapture,
  };
}
