import * as React from 'react';
import { readLooseSources, writeLooseSources } from '../lib/looseSources';
import { requireSource, sourceMutation } from '../lib/taskSources';
import {
  desktop,
  DesktopBridgeError,
  transportCapabilities,
  type ColumnSetupPreview,
  type Capture,
  type DesktopMutation,
  type DesktopSnapshot,
  type EvidenceRange,
  type MoveCompletionDetails,
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
  const snapshotEpoch = React.useRef(0);
  const snapshotReadSequence = React.useRef(0);
  const mutationQueue = React.useRef<Promise<void>>(Promise.resolve());
  const contextSequence = React.useRef(0);
  const selectedSourceRef = React.useRef<string | null>(null);
  const newTaskSourceRef = React.useRef<string | null>(null);
  const [newTaskSource, setNewTaskSourceState] = React.useState<string | null>(null);
  const [sourceBusy, setSourceBusy] = React.useState(false);
  // Loose list reader (Beta) per source: a per-machine preference keyed by
  // project, read fresh on every snapshot so a toggle takes effect at once.
  const looseSourcesRef = React.useRef<string[]>([]);
  const [looseSources, setLooseSourcesState] = React.useState<string[]>([]);
  const looseFor = (projectPath: string | undefined) => (projectPath ? readLooseSources(projectPath) : []);

  const remember = (key: string, value: string | null) => {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* Session choice still works. */ }
  };
  const recalled = (key: string): string | null => {
    try { const value: unknown = JSON.parse(localStorage.getItem(key) ?? 'null'); return typeof value === 'string' ? value : null; } catch { return null; }
  };
  const setNewTaskSource = React.useCallback((source: string | null) => {
    newTaskSourceRef.current = source;
    setNewTaskSourceState(source);
    if (snapshotRef.current) remember(`todo.new-source:${snapshotRef.current.project_path}`, source);
  }, []);

  const applySnapshot = React.useCallback((next: DesktopSnapshot, authoritative = false) => {
    // A revision is a content hash: moving back to a previous priority can
    // legitimately produce a previously seen hash, even a numerically lower one.
    if (authoritative) snapshotEpoch.current += 1;
    const current = snapshotRef.current;
    if (current?.project_path === next.project_path && current.revision === next.revision && current.selected_source === next.selected_source) return false;
    if (current?.project_path !== next.project_path) {
      const saved = recalled(`todo.new-source:${next.project_path}`);
      const valid = next.sources?.some((item) => item.path === saved && item.status === 'managed' && item.writable) ? saved : null;
      newTaskSourceRef.current = valid;
      setNewTaskSourceState(valid);
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

  const refreshSnapshot = React.useCallback(async function readLatest(initial = false): Promise<void> {
    const request = ++snapshotReadSequence.current;
    const epoch = snapshotEpoch.current;
    const context = contextSequence.current;
    try {
      let next = await desktop.snapshot(selectedSourceRef.current, looseSourcesRef.current);
      if (request !== snapshotReadSequence.current || context !== contextSequence.current) return;
      if (initial && next.sources) {
        looseSourcesRef.current = looseFor(next.project_path);
        setLooseSourcesState(looseSourcesRef.current);
        const saved = recalled(`todo.selected-source:${next.project_path}`);
        if (looseSourcesRef.current.length && !saved) {
          next = await desktop.snapshot(null, looseSourcesRef.current);
          if (request !== snapshotReadSequence.current || context !== contextSequence.current) return;
        }
        if (saved && next.sources?.some((item) => item.path === saved)) {
          next = await desktop.snapshot(saved, looseSourcesRef.current);
          if (request !== snapshotReadSequence.current || context !== contextSequence.current) return;
          selectedSourceRef.current = saved;
        }
      }
      // A write completed while this read was in flight. Re-read rather than
      // apply its pre-write result or lose a concurrent external invalidation.
      if (epoch !== snapshotEpoch.current) return readLatest(initial);
      applySnapshot(next);
      setBanner(null);
    } catch (error) {
      if (request !== snapshotReadSequence.current || context !== contextSequence.current) return;
      if (epoch !== snapshotEpoch.current) return readLatest(initial);
      const bridgeError = error instanceof DesktopBridgeError ? error : new DesktopBridgeError(String(error));
      if (initial && !snapshotRef.current) {
        setPhase(bridgeError.kind === 'no_project' ? 'no-project' : 'load-error');
        if (bridgeError.kind === 'no_project') return;
      }
      setBanner(bannerFor(bridgeError));
      if (bridgeError.kind === 'parse') setStatus((value) => ({ ...value, syncState: 'error' }));
    }
  }, [applySnapshot]);

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
          // Notification payloads may arrive out of order. A fresh read, not
          // the content hash or arrival order, determines the current state.
          void refreshSnapshot();
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

      if (!cancelled) await refreshSnapshot(true);
    })();
    void loadCaptures();

    return () => {
      cancelled = true;
      contextSequence.current += 1;
      snapshotReadSequence.current += 1;
      unsubscribe?.();
    };
  }, [refreshSnapshot, loadCaptures]);

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

  const selectSource = React.useCallback(async (source: string | null) => {
    const context = ++contextSequence.current;
    ++snapshotReadSequence.current;
    const previous = selectedSourceRef.current;
    selectedSourceRef.current = source;
    setSourceBusy(true);
    try {
      const next = await desktop.snapshot(source, looseSourcesRef.current);
      if (context !== contextSequence.current) return;
      applySnapshot(next, true);
      remember(`todo.selected-source:${next.project_path}`, source);
      setBanner(null);
    } catch (error) {
      if (context !== contextSequence.current) return;
      selectedSourceRef.current = previous;
      setBanner(bannerFor(error));
    } finally {
      if (context === contextSequence.current) setSourceBusy(false);
    }
  }, [applySnapshot]);

  const acceptWrite = React.useCallback(async function readAfterWrite(next: DesktopSnapshot, context: number): Promise<void> {
    if (context !== contextSequence.current || snapshotRef.current?.project_path !== next.project_path) return;
    if (!next.sources) {
      applySnapshot(next, true);
      return;
    }
    // A delayed write response may predate a watcher update (including revoked
    // permissions). This post-write read shares the refresh sequence/epoch; an
    // intervening newer read or write requires another read, never a hash test.
    const request = ++snapshotReadSequence.current;
    const epoch = snapshotEpoch.current;
    try {
      const displayed = await desktop.snapshot(selectedSourceRef.current, looseSourcesRef.current);
      if (context !== contextSequence.current || snapshotRef.current?.project_path !== next.project_path || displayed.project_path !== next.project_path) return;
      if (request !== snapshotReadSequence.current || epoch !== snapshotEpoch.current) return readAfterWrite(next, context);
      applySnapshot(displayed, true);
    } catch (error) {
      if (context !== contextSequence.current || snapshotRef.current?.project_path !== next.project_path) return;
      if (request !== snapshotReadSequence.current || epoch !== snapshotEpoch.current) return readAfterWrite(next, context);
      throw error;
    }
  }, [applySnapshot]);

  const runMutation = React.useCallback(
    async (key: string, intent: DesktopMutation): Promise<boolean> => {
      const context = contextSequence.current;
      const snapshot = snapshotRef.current;
      try {
        if (!snapshot || sourceBusy) throw new Error('来源正在切换，请稍后重试。');
        const mapped = sourceMutation(snapshot, intent, newTaskSourceRef.current);
        await withBusy(key, () => {
          const write = mutationQueue.current.then(async () => {
            if (context !== contextSequence.current) throw new Error('项目或来源已切换，本次操作未执行。');
            snapshotEpoch.current += 1;
            const authoritative = await desktop.mutate(mapped.intent, mapped.source_path, Boolean(snapshot.sources), snapshot.project_path);
            await acceptWrite(authoritative, context);
          });
          mutationQueue.current = write.catch(() => undefined);
          return write;
        });
        if (context !== contextSequence.current) return false;
        setBanner(null);
        return true;
      } catch (error) {
        if (context !== contextSequence.current) return false;
        setBanner(bannerFor(error));
        const kind = error instanceof DesktopBridgeError ? error.kind : 'unknown';
        if (kind === 'conflict') void refreshSnapshot();
        return false;
      }
    },
    [acceptWrite, refreshSnapshot, withBusy, sourceBusy],
  );

  const applyColumns = React.useCallback(async (preview: ColumnSetupPreview, expectedProjectPath: string): Promise<boolean> => {
    const context = contextSequence.current;
    const assertTarget = () => {
      const current = snapshotRef.current;
      if (context !== contextSequence.current || !current?.sources || sourceBusy || current.project_path !== expectedProjectPath) {
        throw new DesktopBridgeError('项目或来源已切换，请重新打开配置。');
      }
      if (transportCapabilities()?.writable === false) throw new DesktopBridgeError('服务端只读，不能配置任务列。');
      requireSource(current, preview.source_path);
      if (current.tasks.some((task) => task.source_path === preview.source_path && task.source_writable === false)) {
        throw new DesktopBridgeError('此来源只读，不能配置任务列。');
      }
      if (current.columns.some((column) => column.source_path === preview.source_path)) {
        throw new DesktopBridgeError('此来源已定义任务列，请重新读取。', 'conflict');
      }
      const selection = preview.selection;
      if (selection.mode === 'defaults_and_start') {
        const task = current.tasks.find((item) => item.source_path === preview.source_path && item.local_id === selection.task_id);
        if (!task || task.completed || task.column !== null || task.source_writable === false) {
          throw new DesktopBridgeError('任务已变化；仅能开始处理未归列、未完成的可写任务。', 'conflict');
        }
      }
    };
    assertTarget();
    try {
      await withBusy(`columns:${preview.source_path}`, () => {
        const write = mutationQueue.current.then(async () => {
          // Validate again at queue execution, not only when Confirm was clicked.
          assertTarget();
          snapshotEpoch.current += 1;
          try {
            const next = await desktop.applyColumns(preview, expectedProjectPath);
            if (next.project_path !== expectedProjectPath) throw new DesktopBridgeError('写入结果的项目不一致，请重新读取。');
            await acceptWrite(next, context);
          } finally {
            // Also invalidate reads started during a write whose result was lost.
            // A project/source content hash is not a monotonic freshness counter.
            snapshotEpoch.current += 1;
          }
        });
        mutationQueue.current = write.catch(() => undefined);
        return write;
      });
      if (context !== contextSequence.current || snapshotRef.current?.project_path !== expectedProjectPath) return false;
      setBanner(null);
      return true;
    } catch (error) {
      if (context !== contextSequence.current || snapshotRef.current?.project_path !== expectedProjectPath) return false;
      // The dialog owns the failure message and discards its stale confirmation.
      void refreshSnapshot();
      throw error;
    }
  }, [acceptWrite, refreshSnapshot, sourceBusy, withBusy]);

  const chooseProject = React.useCallback(async () => {
    const context = ++contextSequence.current;
    ++snapshotReadSequence.current;
    setSourceBusy(true);
    try {
      snapshotEpoch.current += 1;
      const opened = await withBusy('project', () => desktop.chooseAndOpenProject());
      if (!opened || context !== contextSequence.current) return;
      selectedSourceRef.current = opened.selected_source ?? null;
      applySnapshot(opened, true);
      setBanner(null);
      setPhase('ready');
    } catch (error) {
      if (context !== contextSequence.current) return;
      setBanner(bannerFor(error));
      if (!snapshotRef.current) setPhase('load-error');
    } finally {
      if (context === contextSequence.current) setSourceBusy(false);
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
      const context = contextSequence.current;
      try {
        const snapshot = snapshotRef.current;
        if (!snapshot || sourceBusy) throw new Error('请先选择来源文件。');
        const source = selectedSourceRef.current;
        requireSource(snapshot, source);
        const target = snapshot.columns.find((item) => item.id === column);
        if (!target || (snapshot.sources && target.source_path !== source)) throw new Error('请选择同一来源内的列。');
        const result = await withBusy(`capture:${id}`, () => desktop.captureTriage(id, target.local_id ?? target.id, source, snapshot.project_path));
        if (context !== contextSequence.current) return null;
        if (result.snapshot) await acceptWrite(result.snapshot, context);
        if (context !== contextSequence.current) return null;
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
    [acceptWrite, withBusy, sourceBusy],
  );

  const setLooseSources = React.useCallback(async (paths: string[]) => {
    const root = snapshotRef.current?.project_path;
    if (!root) return;
    writeLooseSources(root, paths);
    looseSourcesRef.current = paths;
    setLooseSourcesState(paths);
    await refreshSnapshot();
  }, [refreshSnapshot]);

  const adoptSource = React.useCallback(async (path: string, baseHash: string): Promise<boolean> => {
    const context = contextSequence.current;
    const root = snapshotRef.current?.project_path;
    const entry = snapshotRef.current?.sources?.find((item) => item.path === path);
    if (!entry || entry.status !== 'unmanaged' || selectedSourceRef.current !== path || sourceBusy) return false;
    const loose = Boolean(entry.loose);
    try {
      const next = await withBusy(`adopt:${path}`, () => desktop.applyAdopt(path, baseHash, root, loose));
      if (loose) {
        // The bullets are real checkboxes now; the Beta reader is no longer
        // needed for this file and must not be applied to its new contents.
        const remaining = looseSourcesRef.current.filter((item) => item !== path);
        writeLooseSources(root!, remaining);
        looseSourcesRef.current = remaining;
        setLooseSourcesState(remaining);
      }
      await acceptWrite(next, context);
      if (context !== contextSequence.current) return false;
      setBanner(null);
      return true;
    } catch (error) {
      if (context !== contextSequence.current) return false;
      setBanner(bannerFor(error));
      throw error;
    }
  }, [acceptWrite, sourceBusy, withBusy]);

  /**
   * Adopts several previewed sources one after another. Each apply is its own
   * atomic write against the hash the person reviewed; the first failure stops
   * the run so nothing is adopted without the diff having been confirmed for
   * the file as it is now. The board is refreshed once at the end.
   */
  const adoptSources = React.useCallback(async (
    items: Array<{ path: string; baseHash: string }>,
    onProgress?: (path: string, outcome: 'adopted' | 'failed', message?: string) => void,
  ): Promise<{ adopted: string[]; failed: { path: string; message: string } | null }> => {
    const context = contextSequence.current;
    const root = snapshotRef.current?.project_path;
    const adopted: string[] = [];
    let latest: DesktopSnapshot | null = null;
    let failed: { path: string; message: string } | null = null;
    for (const { path, baseHash } of items) {
      if (context !== contextSequence.current) break;
      const loose = looseSourcesRef.current.includes(path);
      try {
        latest = await withBusy(`adopt:${path}`, () => desktop.applyAdopt(path, baseHash, root, loose));
        if (loose) {
          const remaining = looseSourcesRef.current.filter((item) => item !== path);
          writeLooseSources(root!, remaining);
          looseSourcesRef.current = remaining;
          setLooseSourcesState(remaining);
        }
        adopted.push(path);
        onProgress?.(path, 'adopted');
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        failed = { path, message };
        onProgress?.(path, 'failed', message);
        break;
      }
    }
    if (latest && context === contextSequence.current) {
      await acceptWrite(latest, context);
      setBanner(null);
    }
    return { adopted, failed };
  }, [acceptWrite, withBusy]);

  return {
    phase,
    snapshot,
    sourceBusy,
    selectSource,
    newTaskSource,
    setNewTaskSource,
    adoptSource,
    adoptSources,
    applyColumns,
    looseSources,
    setLooseSources,
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
      if (next.project_path !== snapshotRef.current?.project_path) return;
      void acceptWrite(next, contextSequence.current);
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
    setBody: (id: string, body: string) =>
      runMutation(`task:${id}`, { type: 'set_body', id, body }),
    setPriority: (id: string, priority: 1 | 2 | 3 | 4 | null, scope: 'task' | 'subtree' = 'task') =>
      runMutation(`task:${id}`, { type: 'set_priority', id, priority, scope }),
    moveTask: (id: string, column: string, completion: MoveCompletionDetails = {}) =>
      runMutation(`task:${id}`, {
        type: 'move_task',
        id,
        column,
        target: { type: 'column_start' },
        ...completion,
      }),
    appendCapture,
    deleteCapture,
    triageCapture,
  };
}
