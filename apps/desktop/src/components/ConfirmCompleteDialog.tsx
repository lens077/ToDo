import * as React from 'react';
import { renderInlineMarkdown } from '../lib/inlineMarkdown';
import type { DesktopTask } from '../lib/desktop';
import { Icon } from './Icon';

interface ConfirmCompleteDialogProps {
  task: DesktopTask;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

/**
 * The one-keystroke gate in front of a tick.
 *
 * A checkbox click writes the file, so a stray click is a real edit. This
 * dialog costs exactly one key: Enter completes, Escape backs out. It carries
 * no form; the note and evidence questions belong to strict mode's outcome
 * dialog, not here.
 */
export function ConfirmCompleteDialog({ task, busy, onCancel, onConfirm }: ConfirmCompleteDialogProps) {
  const confirmRef = React.useRef<HTMLButtonElement>(null);

  React.useEffect(() => {
    confirmRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (busy) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        onCancel();
      } else if (event.key === 'Enter') {
        event.preventDefault();
        onConfirm();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [busy, onCancel, onConfirm]);

  return (
    <div
      className="completion-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <section
        className="confirm-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-complete-title"
        aria-describedby="confirm-complete-body"
      >
        <span className="confirm-panel__mark" aria-hidden="true">
          <Icon name="check" />
        </span>
        <h2 id="confirm-complete-title">标为完成？</h2>
        <p id="confirm-complete-body" className="confirm-panel__body">{renderInlineMarkdown(task.body)}</p>
        <p className="confirm-panel__hint">会写入 TODO.md 并建立还原点；可以在设置里关闭这一步确认。</p>
        <footer className="confirm-panel__actions">
          <button type="button" className="text-action" onClick={onCancel} disabled={busy}>
            取消 <kbd>Esc</kbd>
          </button>
          <button ref={confirmRef} type="button" className="text-action text-action--primary" onClick={onConfirm} disabled={busy}>
            {busy ? '写入中…' : '完成'} <kbd>⏎</kbd>
          </button>
        </footer>
      </section>
    </div>
  );
}
