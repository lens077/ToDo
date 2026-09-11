import * as React from 'react';
import {
  AI_SECTION_ORDER,
  attentionCount,
  projectRoot,
  recordKey,
  scopedRecords,
  type AiRecord,
  type AiScope,
  type AiState,
} from '../lib/ai';
import { desktop, transportCapabilities, type AiToTaskResult, type DesktopSnapshot } from '../lib/desktop';

/** Web polling interval: the browser has no watcher on `~/.todo/ai/`. */
const HTTP_POLL_MS = 5000;

const DEFAULT_VISIBLE: ReadonlySet<AiState> = new Set<AiState>(AI_SECTION_ORDER.filter((state) => state !== 'handled'));

export interface AiTaskTarget {
  column: string | null;
  sourcePath: string | null;
}

interface UseAiFeedOptions {
  projectPath: string | null;
  /** Adopts the board returned by 转为任务 so the new task shows without a second read. */
  onSnapshot: (snapshot: DesktopSnapshot) => void;
}

/**
 * The AI feed as the app sees it: one list, read whenever the store changes,
 * with the scope and pills applied in memory. Actions apply the record the
 * backend returns and then re-read, so a concurrent CLI write is never
 * papered over by an optimistic copy.
 */
export function useAiFeed({ projectPath, onSnapshot }: UseAiFeedOptions) {
  const [records, setRecords] = React.useState<AiRecord[]>([]);
  const [connected, setConnected] = React.useState(false);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<Set<string>>(() => new Set());
  const [visible, setVisible] = React.useState<ReadonlySet<AiState>>(DEFAULT_VISIBLE);
  const root = projectRoot(projectPath);
  const [scopeChoice, setScopeChoice] = React.useState<AiScope>('project');
  // Without a project there is nothing to scope to; the toggle is hidden and
  // the choice is remembered for when a project opens.
  const scope: AiScope = root === null ? 'all' : scopeChoice;
  const readSequence = React.useRef(0);

  const refresh = React.useCallback(async () => {
    const request = ++readSequence.current;
    try {
      const result = await desktop.aiList(null);
      if (request !== readSequence.current) return;
      setRecords(result.records);
      setConnected(result.connected);
      setError(null);
    } catch (caught) {
      if (request !== readSequence.current) return;
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      if (request === readSequence.current) setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;
    void refresh();
    void desktop.subscribeAi(() => {
      if (!cancelled) void refresh();
    }).then((remove) => {
      if (cancelled) remove();
      else unsubscribe = remove;
    }).catch(() => undefined);
    const poll = transportCapabilities()
      ? window.setInterval(() => {
          if (!document.hidden) void refresh();
        }, HTTP_POLL_MS)
      : null;
    return () => {
      cancelled = true;
      readSequence.current += 1;
      unsubscribe?.();
      if (poll !== null) window.clearInterval(poll);
    };
  }, [refresh]);

  const withBusy = React.useCallback(async <T,>(key: string, operation: () => Promise<T>): Promise<T | null> => {
    setBusy((current) => new Set(current).add(key));
    try {
      const result = await operation();
      setError(null);
      return result;
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      return null;
    } finally {
      setBusy((current) => {
        const next = new Set(current);
        next.delete(key);
        return next;
      });
    }
  }, []);

  const applyRecord = React.useCallback((record: AiRecord) => {
    setRecords((current) => {
      const key = recordKey(record);
      return current.some((item) => recordKey(item) === key)
        ? current.map((item) => (recordKey(item) === key ? record : item))
        : [...current, record];
    });
  }, []);

  const ack = React.useCallback(async (record: AiRecord) => {
    const updated = await withBusy(recordKey(record), () => desktop.aiAck(record.source, record.session_id));
    if (updated) applyRecord(updated);
    void refresh();
    return updated !== null;
  }, [applyRecord, refresh, withBusy]);

  const remove = React.useCallback(async (record: AiRecord) => {
    const updated = await withBusy(recordKey(record), () => desktop.aiRemove(record.source, record.session_id));
    if (updated) applyRecord(updated);
    void refresh();
    return updated !== null;
  }, [applyRecord, refresh, withBusy]);

  const toTask = React.useCallback(async (record: AiRecord, target: AiTaskTarget): Promise<AiToTaskResult | null> => {
    const result = await withBusy(recordKey(record), () =>
      desktop.aiToTask(record.source, record.session_id, target.column, target.sourcePath, projectPath ?? undefined));
    if (result) {
      applyRecord(result.record);
      if (result.snapshot) onSnapshot(result.snapshot);
      if (result.snapshot_error) setError(`任务已创建，但列表刷新未完成：${result.snapshot_error}`);
    }
    void refresh();
    return result;
  }, [applyRecord, onSnapshot, projectPath, refresh, withBusy]);

  const open = React.useCallback(async (record: AiRecord) => {
    if (!record.open_url) return false;
    return (await withBusy(`open:${recordKey(record)}`, () => desktop.openExternal(record.open_url!))) !== null;
  }, [withBusy]);

  const toggleVisible = React.useCallback((state: AiState) => {
    setVisible((current) => {
      const next = new Set(current);
      if (next.has(state)) next.delete(state);
      else next.add(state);
      return next;
    });
  }, []);

  const scoped = React.useMemo(() => scopedRecords(records, scope, root), [records, scope, root]);

  return {
    records: scoped,
    /** Records in the other scope exist: tells "nothing here" from "nothing anywhere". */
    totalVisible: React.useMemo(() => scopedRecords(records, 'all', null).length, [records]),
    connected,
    loading,
    error,
    clearError: () => setError(null),
    busy,
    scope,
    canScope: root !== null,
    setScope: setScopeChoice,
    visible,
    toggleVisible,
    attention: attentionCount(scoped),
    refresh,
    ack,
    remove,
    toTask,
    open,
  };
}

export type AiFeedModel = ReturnType<typeof useAiFeed>;
