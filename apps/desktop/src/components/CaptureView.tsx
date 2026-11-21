import * as React from 'react';
import { createPortal } from 'react-dom';
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

interface TriageColumnPickerProps {
  value: string;
  columns: DesktopColumn[];
  disabled: boolean;
  onChange: (column: string) => void;
}

interface PickerPosition {
  left: number;
  width: number;
  top?: number;
  bottom?: number;
}

/**
 * Narrow-screen-safe column picker. The native `<select>` popup is drawn by the
 * OS (blue highlight, 12px rows) and its arrow cannot be placed by the app, so
 * the triage target uses an app-owned listbox: readable type, 44px rows on
 * touch, and the check mark sits at the end of the selected row.
 */
function TriageColumnPicker({ value, columns, disabled, onChange }: TriageColumnPickerProps) {
  const [open, setOpen] = React.useState(false);
  const [activeIndex, setActiveIndex] = React.useState(0);
  const [position, setPosition] = React.useState<PickerPosition | null>(null);
  const triggerRef = React.useRef<HTMLButtonElement>(null);
  const listRef = React.useRef<HTMLDivElement>(null);
  const listId = React.useId();
  const selectedIndex = Math.max(0, columns.findIndex((item) => item.id === value));
  const selected = columns.find((item) => item.id === value);

  const measure = React.useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = Math.max(rect.width, 208);
    const left = Math.min(Math.max(10, rect.left), Math.max(10, window.innerWidth - width - 10));
    const estimated = Math.min(columns.length * 46 + 12, 336);
    const above = window.innerHeight - rect.bottom < estimated + 12 && rect.top > window.innerHeight - rect.bottom;
    setPosition(
      above
        ? { left, width, bottom: window.innerHeight - rect.top + 6 }
        : { left, width, top: rect.bottom + 6 },
    );
  }, [columns.length]);

  const close = React.useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus();
  }, []);

  React.useEffect(() => {
    if (!open) return;
    measure();
    const dismiss = () => setOpen(false);
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (listRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      dismiss();
    };
    const onScroll = (event: Event) => {
      // Scrolling inside the list (to reach the active row) must not close it.
      if (event.target instanceof Node && listRef.current?.contains(event.target)) return;
      dismiss();
    };
    window.addEventListener('resize', dismiss);
    window.addEventListener('scroll', onScroll, true);
    document.addEventListener('pointerdown', onPointerDown);
    return () => {
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('scroll', onScroll, true);
      document.removeEventListener('pointerdown', onPointerDown);
    };
  }, [open, measure]);

  React.useEffect(() => {
    if (!open || !position) return;
    listRef.current?.focus();
    listRef.current?.querySelector<HTMLElement>('[data-active="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [open, position, activeIndex]);

  const pick = (column: string) => {
    onChange(column);
    close(true);
  };

  const onListKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Escape' || event.key === 'Tab') {
      if (event.key === 'Escape') event.preventDefault();
      close(true);
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveIndex((index) => Math.min(columns.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1))));
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      setActiveIndex(event.key === 'Home' ? 0 : columns.length - 1);
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      const column = columns[activeIndex];
      if (column) pick(column.id);
    }
  };

  return (
    <div className="column-picker">
      <button
        ref={triggerRef}
        type="button"
        className={`column-picker__trigger${open ? ' is-open' : ''}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-label="选择目标列"
        disabled={disabled || !columns.length}
        onClick={() => (open ? close(false) : (setActiveIndex(selectedIndex), setOpen(true)))}
        onKeyDown={(event) => {
          if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
          event.preventDefault();
          if (open) return;
          setActiveIndex(selectedIndex);
          setOpen(true);
        }}
      >
        <span className="column-picker__value">{selected?.name ?? '选择列'}</span>
        <Icon name="chevron" />
      </button>
      {open && position
        ? createPortal(
            <div
              ref={listRef}
              id={listId}
              className="column-picker__menu"
              role="listbox"
              aria-label="选择目标列"
              tabIndex={-1}
              style={{ left: position.left, width: position.width, top: position.top, bottom: position.bottom }}
              onKeyDown={onListKeyDown}
            >
              {columns.map((column, index) => (
                <div
                  key={column.id}
                  role="option"
                  aria-selected={column.id === value}
                  data-active={index === activeIndex}
                  className={`column-picker__option${column.id === value ? ' is-selected' : ''}${index === activeIndex ? ' is-active' : ''}`}
                  onPointerEnter={() => setActiveIndex(index)}
                  onClick={() => pick(column.id)}
                >
                  <span>{column.name}</span>
                  {column.id === value && <Icon name="check" />}
                </div>
              ))}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
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
          <TriageColumnPicker
            value={column}
            columns={columns}
            disabled={readOnly || !columns.length || busy}
            onChange={setColumn}
          />
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
        onClick={() => {
          if (window.confirm('删除这条随记？删除后不可恢复。')) onDelete(capture.id);
        }}
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
