import * as React from 'react';
import type { Capture, DesktopColumn, DesktopSnapshot, TriageResult } from '../lib/desktop';
import type { PartialTriageState } from '../hooks/useDesktop';
import { formatCaptureTime, shortId } from '../lib/format';
import { Icon } from './Icon';

interface CaptureViewProps {
  triageUnavailable?: boolean;
  snapshot: DesktopSnapshot | null;
  captures: Capture[];
  capturesLoading: boolean;
  busyKeys: Set<string>;
  partialTriages: Record<string, PartialTriageState>;
  readOnly: boolean;
  onAppend: (text: string) => Promise<boolean>;
  onDelete: (id: string) => Promise<boolean>;
  onTriage: (id: string, column: string) => Promise<TriageResult | null>;
}

interface CaptureItemProps {
  capture: Capture;
  columns: DesktopColumn[];
  busy: boolean;
  readOnly: boolean;
  partial?: PartialTriageState;
  onDelete: (id: string) => Promise<boolean>;
  onTriage: (id: string, column: string) => Promise<TriageResult | null>;
}

function CaptureItem({ capture, columns, busy, readOnly, partial, onDelete, onTriage }: CaptureItemProps) {
  const [column, setColumn] = React.useState(columns.find((item) => !item.done)?.id ?? columns[0]?.id ?? '');

  React.useEffect(() => {
    if (!columns.some((item) => item.id === column)) {
      setColumn(columns.find((item) => !item.done)?.id ?? columns[0]?.id ?? '');
    }
  }, [column, columns]);

  return (
    <article className={`capture-item${partial ? ' capture-item--partial' : ''}${busy ? ' is-busy' : ''}`}>
      <header className="capture-item__meta">
        <span>{formatCaptureTime(capture.created_at)}</span>
        <code>{capture.id}</code>
      </header>
      <p className="capture-item__text">{capture.text}</p>

      {partial ? (
        <div className="partial-success" role="status">
          <Icon name="alert" />
          <div>
            <strong>任务已创建，随记仍保留</strong>
            <p>
              Project Task <code>{shortId(partial.taskId)}</code> 已经写入。请只删除这条 Capture，不要再次分拣。
            </p>
            <small>{partial.message}</small>
          </div>
        </div>
      ) : (
        <div className="capture-item__triage">
          <label>
            <span className="sr-only">选择目标列</span>
            <select value={column} onChange={(event) => setColumn(event.target.value)} disabled={readOnly || !columns.length || busy}>
              {columns.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
          </label>
          <button
            className="text-action text-action--primary"
            type="button"
            disabled={readOnly || !column || busy}
            onClick={() => void onTriage(capture.id, column)}
          >
            分拣为任务 <Icon name="arrow" />
          </button>
        </div>
      )}

      <button
        className="icon-action capture-item__delete"
        type="button"
        disabled={readOnly || busy}
        aria-label={`删除 Capture：${capture.text}`}
        title="删除 Capture"
        onClick={() => void onDelete(capture.id)}
      >
        <Icon name="trash" />
      </button>
      {busy && <span className="capture-item__saving">处理中</span>}
    </article>
  );
}

export function CaptureView({
  snapshot,
  captures,
  capturesLoading,
  busyKeys,
  partialTriages,
  readOnly,
  onAppend,
  onDelete,
  onTriage,
  triageUnavailable = false,
}: CaptureViewProps) {
  const [draft, setDraft] = React.useState('');
  const submit = async () => {
    if (readOnly || !draft.trim() || busyKeys.has('capture:new')) return;
    const value = draft;
    if (await onAppend(value)) setDraft('');
  };

  return (
    <section className="capture-view" aria-labelledby="capture-title">
      <header className="view-heading view-heading--capture">
        <div className="view-heading__title">
          <h1 id="capture-title">随记，不急着承诺</h1>
          <p className="view-heading__meta">
            <span>{captures.length} 条 Capture</span>
            <span aria-hidden="true">·</span>
            <span>个人收件箱</span>
          </p>
        </div>
        <p className="view-heading__note">Capture 是个人原始文本；分拣后才成为当前项目的工作。</p>
      </header>

      {triageUnavailable && <p className="source-unavailable">分拣依赖单个可写来源，请先在「任务来源」选择已管理文件。个人随记仍可记录。</p>}
      <form
        className="inline-capture"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <span className="inline-capture__mark">+</span>
        <label htmlFor="inline-capture-text">记下一件还没想清楚的事</label>
        <textarea
          id="inline-capture-text"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              void submit();
            }
          }}
          placeholder={readOnly ? '只读模式下不能新增随记' : '先记原话，稍后再分拣……'}
          disabled={readOnly}
          rows={2}
        />
        <div className="inline-capture__footer">
          <span>
            <kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>Enter</kbd>
          </span>
          <button className="solid-action" type="submit" disabled={readOnly || !draft.trim() || busyKeys.has('capture:new')}>
            {readOnly ? '只读' : busyKeys.has('capture:new') ? '保存中' : '存入随记'}
          </button>
        </div>
      </form>

      <div className="capture-ledger">
        <div className="capture-ledger__rule">
          <span>CAPTURE LEDGER</span>
          <span>{snapshot ? `TRIAGE → ${snapshot.columns.length} COLUMNS` : 'TRIAGE PAUSED / NO PROJECT'}</span>
        </div>
        {capturesLoading ? (
          <div className="capture-loading" aria-label="正在读取随记">
            <span />
            <span />
            <span />
          </div>
        ) : captures.length ? (
          <div className="capture-list">
            {captures.map((capture, index) => (
              <div className="capture-item-wrap" style={{ '--capture-index': index } as React.CSSProperties} key={capture.id}>
                <CaptureItem
                  capture={capture}
                  columns={triageUnavailable ? [] : snapshot?.columns ?? []}
                  busy={busyKeys.has(`capture:${capture.id}`)}
                  readOnly={readOnly}
                  partial={partialTriages[capture.id]}
                  onDelete={onDelete}
                  onTriage={onTriage}
                />
              </div>
            ))}
          </div>
        ) : (
          <div className="capture-empty">
            <span className="capture-empty__circle"><Icon name="check" /></span>
            <h2>随记已经清空</h2>
            <p>没有等待分拣的 Capture。新的想法会从全局速记窗口汇入这里。</p>
          </div>
        )}
      </div>
    </section>
  );
}
