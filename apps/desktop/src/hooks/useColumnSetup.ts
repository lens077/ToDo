import * as React from 'react';
import { desktop, DesktopBridgeError, type ColumnSetupOptions, type ColumnSetupPreview, type ColumnSetupSelection } from '../lib/desktop';

export interface ColumnSetupDialogProps {
  projectPath: string;
  sourcePath: string;
  readOnly: boolean;
  onApply: (preview: ColumnSetupPreview, expectedProjectPath: string) => Promise<boolean>;
  onClose: () => void;
  restoreFocus: HTMLElement | null;
  /** Fixed, source-local task intent for a direct drop into work in flight. */
  startTask?: { localId: string; body: string };
}

/** Each read belongs to this dialog lifetime and to the current choice. */
export function useColumnSetup({ projectPath, sourcePath, readOnly, onApply, onClose, startTask }: ColumnSetupDialogProps) {
  const [options, setOptions] = React.useState<ColumnSetupOptions | null>(null);
  const [selection, setSelection] = React.useState<ColumnSetupSelection>(() => startTask
    ? { mode: 'defaults_and_start', task_id: startTask.localId }
    : { mode: 'defaults' });
  const [preview, setPreview] = React.useState<ColumnSetupPreview | null>(null);
  const [phase, setPhase] = React.useState<'loading' | 'select' | 'previewing' | 'applying'>('loading');
  const [error, setError] = React.useState<string | null>(null);
  const [needsRepreview, setNeedsRepreview] = React.useState(false);
  const request = React.useRef(0);
  const applying = React.useRef(false);
  const autoPreviewed = React.useRef(false);

  const load = React.useCallback(async () => {
    const id = ++request.current;
    setPhase('loading');
    setError(null);
    setOptions(null);
    setPreview(null);
    try {
      const next = await desktop.columnSetupOptions(sourcePath, projectPath);
      if (request.current === id) setOptions(next);
    } catch (cause) {
      if (request.current === id) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (request.current === id) setPhase('select');
    }
  }, [projectPath, sourcePath]);

  React.useEffect(() => {
    void load();
    return () => { request.current += 1; };
  }, [load]);

  React.useEffect(() => {
    if (!readOnly || applying.current) return;
    // Revoking permission invalidates a preview even if permission later returns.
    request.current += 1;
    setPreview(null);
    setPhase('select');
  }, [readOnly]);

  const change = (next: ColumnSetupSelection) => {
    if (applying.current || startTask) return;
    request.current += 1;
    setSelection(next);
    setPreview(null);
    setPhase('select');
    setError(null);
  };
  const canPreview = Boolean(options && !options.configured && !readOnly
    && phase !== 'loading' && phase !== 'previewing' && phase !== 'applying'
    && (selection.mode !== 'existing' || selection.headings.length > 0));

  const prepare = React.useCallback(async () => {
    if (!canPreview || applying.current) return;
    const id = ++request.current;
    setPreview(null);
    setError(null);
    setPhase('previewing');
    try {
      const next = await desktop.previewColumns(sourcePath, selection, projectPath);
      if (request.current !== id) return;
      setPreview(next);
      setNeedsRepreview(false);
    } catch (cause) {
      if (request.current === id) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (request.current === id) setPhase('select');
    }
  }, [canPreview, projectPath, selection, sourcePath]);

  React.useEffect(() => {
    if (!startTask || !canPreview || autoPreviewed.current) return;
    autoPreviewed.current = true;
    void prepare();
  }, [canPreview, prepare, startTask]);

  const apply = async () => {
    if (!preview || readOnly || applying.current) return;
    const id = ++request.current;
    applying.current = true;
    setPhase('applying');
    setError(null);
    try {
      const written = await onApply(preview, projectPath);
      if (request.current !== id) return;
      if (written) onClose();
      else throw new Error('项目或来源已变化，请重新打开配置。');
    } catch (cause) {
      if (request.current !== id) return;
      setPreview(null);
      setNeedsRepreview(true);
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(cause instanceof DesktopBridgeError && cause.kind === 'conflict'
        ? `文件已变化，本次配置未写入。${message} 请重新预览差异。`
        : `未能确认配置结果。${message} 请重新读取并预览，不要重复提交旧预览。`);
    } finally {
      applying.current = false;
      if (request.current === id) setPhase('select');
    }
  };

  const close = () => {
    if (applying.current) return;
    request.current += 1;
    onClose();
  };
  return { options, selection, preview, phase, error, needsRepreview, canPreview, change, load, prepare, apply, close };
}
