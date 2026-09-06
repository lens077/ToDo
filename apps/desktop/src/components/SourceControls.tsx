import * as React from 'react';
import { desktop, type AdoptPreview, type DesktopSnapshot, type TaskSource } from '../lib/desktop';
import { DiffView } from './DiffView';
import { Icon } from './Icon';

export function SourceControls({ snapshot, busy, discoveredCount, pendingCount, onSelect, onAdjustEnrollment }: {
  /** The board's snapshot: only enrolled sources. */
  snapshot: DesktopSnapshot;
  busy: boolean;
  /** Everything discovery found, enrolled or not. */
  discoveredCount: number;
  /** Sources found since the last enrollment decision. */
  pendingCount: number;
  onSelect: (path: string | null) => void;
  onAdjustEnrollment?: () => void;
}) {
  const [hintsOpen, setHintsOpen] = React.useState(false);
  if (!snapshot.sources) return null;
  const sources = snapshot.sources;
  const readonlyCount = sources.filter((source) => source.status !== 'managed' || !source.writable).length;
  const warnings = snapshot.warnings ?? [];
  const summary = busy
    ? '正在读取来源…'
    : discoveredCount > sources.length
      ? `已录入 ${sources.length} / ${discoveredCount} 个来源${readonlyCount ? ` · ${readonlyCount} 个只读或待处理` : ''}`
      : `${sources.length} 个来源${readonlyCount ? ` · ${readonlyCount} 个只读或待处理` : ''}`;
  return <section className="source-controls" aria-label="项目任务来源">
    <div className="source-controls__row">
      <label className="source-controls__select">
        <span>任务来源</span>
        <select aria-label="任务来源" value={snapshot.selected_source ?? ''} disabled={busy} onChange={(event) => onSelect(event.target.value || null)}>
          <option value="">所有来源</option>
          {sources.map((source) => <option key={source.path} value={source.path}>{source.path} · {source.status === 'error' ? '读取错误' : source.status === 'unmanaged' ? '未接管' : source.writable ? '已管理' : '只读'}</option>)}
        </select>
      </label>
      <span className="source-controls__summary">{summary}</span>
      {snapshot.selected_source === null && <span className="source-controls__hint">聚合浏览；新增需明确选择来源，扫描与分拣请先选单文件。</span>}
      <div className="source-controls__tools">
        {pendingCount > 0 && onAdjustEnrollment && (
          <button type="button" className="source-controls__pending" onClick={onAdjustEnrollment}>
            <span className="source-controls__dot" aria-hidden="true" />
            发现 {pendingCount} 个新来源，尚未录入
          </button>
        )}
        {onAdjustEnrollment && discoveredCount > 1 && (
          <button type="button" className="source-controls__action" onClick={onAdjustEnrollment}>
            <Icon name="folder" />
            调整录入…
          </button>
        )}
        {warnings.length > 0 && (
          <button
            type="button"
            className={`source-controls__action source-controls__hints${hintsOpen ? ' is-open' : ''}`}
            aria-expanded={hintsOpen}
            aria-controls="source-discovery-hints"
            onClick={() => setHintsOpen((open) => !open)}
          >
            <Icon name="alert" />
            发现提示
            <span className="source-controls__badge">{warnings.length}</span>
            <Icon name="chevron" className="source-controls__chevron" />
          </button>
        )}
      </div>
    </div>
    {warnings.length > 0 && hintsOpen && (
      <ul id="source-discovery-hints" className="source-hints" aria-label="发现提示">
        {warnings.map((warning, index) => <li key={index}><code>{warning}</code></li>)}
      </ul>
    )}
  </section>;
}

export function SourcePreview({ source, readOnly, onAdopt }: {
  source: TaskSource;
  readOnly: boolean;
  onAdopt: (path: string, hash: string) => Promise<boolean>;
}) {
  const [preview, setPreview] = React.useState<AdoptPreview | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const alive = React.useRef(true);
  React.useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const prepare = async () => {
    if (readOnly || source.status !== 'unmanaged' || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await desktop.previewAdopt(source.path);
      if (!alive.current) return;
      if (next.source_path !== source.path) throw new Error('预览来源不一致，未写入文件。');
      setPreview(next);
    } catch (cause) {
      if (alive.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally { if (alive.current) setBusy(false); }
  };
  const apply = async () => {
    if (readOnly || !preview || busy) return;
    setBusy(true);
    setError(null);
    try {
      const ok = await onAdopt(preview.source_path, preview.base_hash);
      if (alive.current && !ok) setError('项目或来源已变化，请重新预览；未确认当前文件的写入结果。');
    } catch (cause) {
      if (alive.current) {
        setPreview(null);
        setError(`接管未完成，文件未按此预览写入。${cause instanceof Error ? cause.message : String(cause)} 请重新读取并预览。`);
      }
    } finally { if (alive.current) setBusy(false); }
  };
  return <section className="source-preview" aria-label="来源只读预览">
    <h2>{source.path}</h2>
    <p>{source.status === 'error' ? '此来源读取或解析失败；仅展示原文，不写入。' : source.status === 'unmanaged' ? '发现普通任务清单，当前只读。先查看接管 diff，确认后才加入工具管理。' : '此来源为只读，不能写入。'}</p>
    {source.error && <p role="alert" className="review-error">{source.error}</p>}
    {error && <p role="alert" className="review-error">{error}</p>}
    {preview ? <>
      <h3>接管差异预览</h3>
      <DiffView diff={preview.diff} leftLabel={source.path} rightLabel="接管后" />
      <p>只确认当前来源；文件若已在外部变化，服务会拒绝写入。</p>
      <div className="review-actions"><button className="text-action" disabled={busy} onClick={() => setPreview(null)}>取消接管</button><button className="solid-action" disabled={readOnly || busy} onClick={() => void apply()}>{busy ? '确认中…' : '确认接管并写入'}</button></div>
    </> : <>
      <pre className="source-preview__raw" aria-label="来源原文">{source.preview || '服务未提供可显示的原文。'}</pre>
      {source.preview_truncated && <p className="source-preview__note">原文只显示前 16 KB；接管差异与写入使用完整文件。</p>}
      {source.status === 'unmanaged' && <button className="solid-action" disabled={readOnly || busy} onClick={() => void prepare()}>{busy ? '生成预览…' : '预览接管差异'}</button>}
    </>}
    {readOnly && <p>服务当前只读，不能接管文件。</p>}
  </section>;
}
