import * as React from 'react';
import { createPortal } from 'react-dom';
import type { DesktopTask } from '../lib/desktop';
import { renderInlineMarkdown } from '../lib/inlineMarkdown';

interface TaskBodyEditorProps {
  task: DesktopTask;
  anchor: HTMLElement;
  readOnly: boolean;
  busy: boolean;
  onSave: (body: string) => Promise<boolean>;
  /** `false` when focus already moved elsewhere, such as an outside click. */
  onClose: (restoreFocus?: boolean) => void;
}

/** Share of the content region the editor takes, so it scales with the window. */
const WIDTH_RATIO = 0.62;
const HEIGHT_RATIO = 0.52;

/** A task body is one source line: pasted line breaks become spaces. */
function singleLine(value: string): string {
  return value.replace(/\s*[\r\n]+\s*/g, ' ');
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

/**
 * Centres the editor in the visible content region (the workspace beside the
 * sidebar, below the command bar) and sizes it in proportion to that region,
 * so it grows and shrinks with the window instead of hugging the card.
 */
function placement(anchor: HTMLElement) {
  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = document.documentElement.clientHeight;
  const region = anchor.closest('main')?.getBoundingClientRect();
  const top = Math.max(region?.top ?? 0, 0);
  const left = Math.max(region?.left ?? 0, 0);
  const right = Math.min(region?.right ?? viewportWidth, viewportWidth);
  const bottom = Math.min(region?.bottom ?? viewportHeight, viewportHeight);
  const regionWidth = right - left;
  const regionHeight = bottom - top;
  const width = Math.min(clamp(regionWidth * WIDTH_RATIO, 320, 880), viewportWidth - 16);
  const height = Math.min(clamp(regionHeight * HEIGHT_RATIO, 240, 640), viewportHeight - 16);
  return {
    left: clamp(left + (regionWidth - width) / 2, 8, viewportWidth - width - 8),
    top: clamp(top + (regionHeight - height) / 2, 8, viewportHeight - height - 8),
    width,
    height,
  };
}

/**
 * Edits the raw Markdown of one task line in a popover over the card.
 *
 * The card itself is a drag handle, so its text cannot be selected; the
 * popover is where the text becomes selectable and, for writable sources,
 * editable. Saving rewrites only the body span of that line in the source
 * file; checkbox, metadata tokens and the `↳` note stay byte-for-byte.
 */
export function TaskBodyEditor({ task, anchor, readOnly, busy, onSave, onClose }: TaskBodyEditorProps) {
  const [value, setValue] = React.useState(task.body);
  const [position, setPosition] = React.useState(() => placement(anchor));
  const panelRef = React.useRef<HTMLDivElement>(null);
  const fieldRef = React.useRef<HTMLTextAreaElement>(null);
  const previousBody = React.useRef(task.body);
  const trimmed = value.trim();
  const dirty = trimmed !== task.body;
  const canSave = !readOnly && !busy && dirty && trimmed.length > 0;
  const titleId = React.useId();

  React.useEffect(() => {
    const field = fieldRef.current;
    if (!field) return;
    field.focus({ preventScroll: true });
    const end = field.value.length;
    field.setSelectionRange(end, end);
  }, []);

  React.useEffect(() => {
    const reposition = () => {
      if (!anchor.isConnected) {
        onClose(false);
        return;
      }
      setPosition(placement(anchor));
    };
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    return () => {
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
    };
  }, [anchor, onClose]);

  // An outside click closes an untouched editor; unsaved text is never
  // thrown away silently, it needs an explicit 取消 or Escape.
  React.useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      // The anchor toggles the editor itself; closing here would reopen it.
      if (panelRef.current?.contains(target) || anchor.contains(target)) return;
      if (!dirty || readOnly) onClose(false);
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    return () => document.removeEventListener('pointerdown', onPointerDown, true);
  }, [anchor, dirty, readOnly, onClose]);

  // Keep the popover in sync if the file changes underneath an untouched editor.
  React.useEffect(() => {
    setValue((current) => (current.trim() === previousBody.current ? task.body : current));
    previousBody.current = task.body;
  }, [task.body]);

  const save = async () => {
    if (!dirty) {
      onClose();
      return;
    }
    if (!canSave) return;
    if (await onSave(trimmed)) onClose();
  };

  return createPortal(
    <div
      ref={panelRef}
      className="body-editor"
      role="dialog"
      aria-labelledby={titleId}
      style={{ left: position.left, top: position.top, width: position.width, height: position.height }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Escape') {
          event.preventDefault();
          onClose();
        }
      }}
      onDragStart={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="body-editor__head">
        <span id={titleId}>{readOnly ? '任务正文 · 只读' : '编辑任务正文'}</span>
        {task.source_path !== undefined && <span className="body-editor__source" title={task.source_path}>{task.source_path}</span>}
      </div>
      <textarea
        ref={fieldRef}
        className="body-editor__field"
        value={value}
        readOnly={readOnly}
        disabled={busy}
        spellCheck={false}
        aria-label="任务正文 Markdown"
        onChange={(event) => setValue(singleLine(event.target.value))}
        onKeyDown={(event) => {
          if (event.key !== 'Enter') return;
          event.preventDefault();
          // Enter while an IME is composing confirms the candidate, not the edit.
          if (event.nativeEvent.isComposing || event.keyCode === 229) return;
          if (!readOnly) void save();
        }}
      />
      {!readOnly && dirty && trimmed && (
        <div className="body-editor__preview" aria-label="渲染预览">
          {renderInlineMarkdown(trimmed)}
        </div>
      )}
      <div className="body-editor__foot">
        <small className="body-editor__hint">
          {readOnly
            ? '来源只读：可选择并复制文字。'
            : '支持 **粗体**、`代码`、*斜体*、~~删除线~~、[链接](url)。回车保存，Esc 取消。'}
        </small>
        <div className="body-editor__actions">
          <button type="button" className="body-editor__cancel" onClick={() => onClose()} disabled={busy}>
            {readOnly ? '关闭' : '取消'}
          </button>
          {!readOnly && (
            <button type="button" className="body-editor__save" onClick={() => void save()} disabled={!canSave}>
              {busy ? '写入中' : '保存'}
            </button>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}
