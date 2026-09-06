import * as React from 'react';
import {
  desktop,
  type DesktopSnapshot,
  type HistoryEntry,
  type ProposedRewrite,
  type ScanPreview,
} from '../lib/desktop';
import { DiffView } from './DiffView';

/**
 * What the user is currently being asked to confirm.
 *
 * A rewrite is never applied straight from an action: it lands here first, the
 * diff is shown, and only an explicit confirm writes. `op` labels the resulting
 * restore point so history reads as a sequence of causes, not anonymous edits.
 */
interface PendingRewrite {
  rewrite: ProposedRewrite;
  op: string;
  title: string;
  leftLabel: string;
  rightLabel: string;
  version?: number;
}

interface ReviewViewProps {
  snapshot: DesktopSnapshot | null;
  readOnly: boolean;
  onApplied: (snapshot: DesktopSnapshot) => void;
}

type Busy = 'scan' | 'import' | 'history' | 'apply' | null;

export function ReviewView({ snapshot, readOnly, onApplied }: ReviewViewProps) {
  const [markdown, setMarkdown] = React.useState(true);
  const [comments, setComments] = React.useState(true);
  const [scan, setScan] = React.useState<ScanPreview | null>(null);
  const [history, setHistory] = React.useState<HistoryEntry[]>([]);
  const [pending, setPending] = React.useState<PendingRewrite | null>(null);
  const [busy, setBusy] = React.useState<Busy>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [prompt, setPrompt] = React.useState<string | null>(null);

  const column = snapshot?.columns[0]?.local_id ?? snapshot?.columns[0]?.id ?? 'todo';
  const sourcePath = snapshot?.selected_source ?? null;
  const alive = React.useRef(true);
  React.useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);

  const guard = React.useCallback(async (key: Busy, run: () => Promise<void>) => {
    setBusy(key);
    setError(null);
    try {
      await run();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, []);

  const runScan = () =>
    guard('scan', async () => {
      setPending(null);
      setScan(await desktop.scanRepository(markdown, comments, null, sourcePath));
    });

  const previewImport = () => {
    if (readOnly) return Promise.resolve();
    return guard('import', async () => {
      const preview = await desktop.scanRepository(markdown, comments, column, sourcePath);
      setScan(preview);
      if (!preview.rewrite) return;
      setPending({
        rewrite: preview.rewrite,
        op: 'scan-import',
        title: '导入扫描结果',
        leftLabel: '当前 TODO.md',
        rightLabel: '导入后',
      });
    });
  };

  const loadHistory = () =>
    guard('history', async () => {
      setHistory(await desktop.historyList(sourcePath));
    });

  const previewRestore = (version: number) => {
    if (readOnly) return Promise.resolve();
    return guard('history', async () => {
      const rewrite = await desktop.historyPreview(version, sourcePath);
      setPending({
        rewrite,
        op: 'restore',
        version,
        title: `恢复到 v${version}`,
        leftLabel: `v${version}（还原点）`,
        rightLabel: '当前 TODO.md',
      });
    });
  };

  const confirm = () => {
    if (readOnly || !pending) return;
    void guard('apply', async () => {
      const next = await desktop.applyRewrite(pending.rewrite, pending.op, sourcePath, column, pending.version, snapshot?.project_path);
      if (!alive.current) return;
      onApplied(next);
      setPending(null);
      setScan(null);
      setHistory(await desktop.historyList(sourcePath));
    });
  };

  const showPrompt = () =>
    guard('scan', async () => {
      setPrompt(await desktop.alignmentPrompt(sourcePath));
    });

  if (!snapshot) {
    return <p className="review-empty">请先打开一个项目。</p>;
  }

  const report = scan?.report;
  const found = report ? report.loose_checkboxes.length + report.code_comments.length : 0;

  return (
    <div className="review">
      {error && (
        <p className="review-error" role="alert">
          {error}
        </p>
      )}

      {pending ? (
        <section className="review-panel">
          <header className="review-panel__head">
            <h2>{pending.title}</h2>
            <p>确认后才会写入文件；写入前会自动建立还原点。</p>
          </header>
          <DiffView
            diff={pending.rewrite.diff}
            leftLabel={pending.leftLabel}
            rightLabel={pending.rightLabel}
          />
          <div className="review-actions">
            <button type="button" className="text-action" onClick={() => setPending(null)}>
              取消
            </button>
            <button
              type="button"
              className="text-action text-action--primary"
              onClick={confirm}
              disabled={readOnly || busy === 'apply'}
            >
              {busy === 'apply' ? '写入中…' : '确认并写入'}
            </button>
          </div>
        </section>
      ) : (
        <>
          <section className="review-panel">
            <header className="review-panel__head">
              <h2>扫描全仓</h2>
              <p>找出仓库里还没被跟踪的工作。扫描只读，不会改动任何文件。</p>
            </header>
            <div className="review-options">
              <label>
                <input
                  type="checkbox"
                  checked={markdown}
                  onChange={(event) => setMarkdown(event.target.checked)}
                />
                其他 Markdown 里的 checkbox
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={comments}
                  onChange={(event) => setComments(event.target.checked)}
                />
                代码注释 TODO / FIXME
              </label>
            </div>
            <div className="review-actions">
              <button
                type="button"
                className="text-action"
                onClick={runScan}
                disabled={busy !== null || (!markdown && !comments)}
              >
                {busy === 'scan' ? '扫描中…' : '扫描'}
              </button>
              <button
                type="button"
                className="text-action"
                onClick={previewImport}
                disabled={readOnly || busy !== null || found === 0}
                title={readOnly ? '只读模式下不能导入' : found === 0 ? '先扫描出结果才能导入' : undefined}
              >
                预览导入
              </button>
              <button type="button" className="text-action" onClick={showPrompt} disabled={busy !== null}>
                对齐提示词
              </button>
            </div>

            {report && (
              <div className="review-results">
                <p className="review-stats">
                  扫描 {report.stats.files_scanned} 个文件，跳过 {report.stats.files_skipped} 个，
                  已跟踪而过滤 {report.stats.duplicates_filtered} 条 — 发现 {found} 条
                </p>
                <ul className="review-list">
                  {report.loose_checkboxes.map((item) => (
                    <li key={`md-${item.path}-${item.line}`}>
                      <span className="review-tag">md</span>
                      <code>
                        {item.path}:{item.line}
                      </code>
                      <span>{item.text}</span>
                    </li>
                  ))}
                  {report.code_comments.map((item) => (
                    <li key={`c-${item.path}-${item.line}`}>
                      <span className="review-tag">{item.marker.toLowerCase()}</span>
                      <code>
                        {item.path}:{item.line}
                      </code>
                      <span>{item.text}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </section>

          {prompt && (
            <section className="review-panel">
              <header className="review-panel__head">
                <h2>对齐提示词</h2>
                <p>交给 agent 执行，再用 `todo align --review` 校验它的回复。</p>
              </header>
              <textarea className="review-prompt" readOnly value={prompt} rows={12} />
            </section>
          )}

          <section className="review-panel">
            <header className="review-panel__head">
              <h2>版本历史</h2>
              <p>每次写入前自动记录还原点。恢复本身也会成为新的还原点。</p>
            </header>
            <div className="review-actions">
              <button type="button" className="text-action" onClick={loadHistory} disabled={busy !== null}>
                {busy === 'history' ? '读取中…' : '刷新历史'}
              </button>
            </div>
            {history.length > 0 && (
              <ul className="review-list review-list--history">
                {history.map((entry) => (
                  <li key={entry.version}>
                    <span className="review-tag">v{entry.version}</span>
                    <code>{entry.op}</code>
                    <span>{new Date(entry.at).toLocaleString()}</span>
                    <button
                      type="button"
                      className="text-action"
                      onClick={() => previewRestore(entry.version)}
                      disabled={readOnly || busy !== null}
                      title={readOnly ? '只读模式下不能恢复' : undefined}
                    >
                      预览恢复
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}
