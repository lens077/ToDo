import * as React from 'react';
import { renderInlineMarkdown } from '../lib/inlineMarkdown';
import { createPortal } from 'react-dom';
import type { DesktopTask, TaskPriority } from '../lib/desktop';

interface PriorityScopeDialogProps {
  task: DesktopTask;
  descendantCount: number;
  priority: TaskPriority;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (scope: 'task' | 'subtree') => void;
}

const names: Record<Exclude<TaskPriority, null>, string> = {
  1: '最高',
  2: '高',
  3: '中',
  4: '低',
};

export function PriorityScopeDialog({
  task,
  descendantCount,
  priority,
  busy,
  onCancel,
  onConfirm,
}: PriorityScopeDialogProps) {
  const dialogRef = React.useRef<HTMLElement>(null);
  const previousFocus = React.useRef<HTMLElement | null>(null);
  React.useEffect(() => {
    previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialogRef.current?.querySelector<HTMLButtonElement>('button')?.focus();
    return () => previousFocus.current?.focus();
  }, []);

  const destination = priority === null ? '未定级' : `!${priority} ${names[priority]}`;
  return createPortal(
    <div
      className="dialog-backdrop priority-scope-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && !busy) onCancel();
        if (event.key === 'Tab') {
          const buttons = Array.from(dialogRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []);
          if (!buttons.length) return;
          const edge = event.shiftKey ? buttons[0] : buttons.at(-1);
          if (document.activeElement === edge) {
            event.preventDefault();
            (event.shiftKey ? buttons.at(-1) : buttons[0])?.focus();
          }
        }
      }}
    >
      <section ref={dialogRef} className="priority-scope-dialog" role="dialog" aria-modal="true" aria-labelledby="priority-scope-title">
        <h2 id="priority-scope-title">这个任务包含子任务</h2>
        <p>将「{renderInlineMarkdown(task.body)}」移到 {destination}。选择这次要改动的范围。</p>
        <div className="priority-scope-dialog__actions">
          <button type="button" className="button-primary" disabled={busy} onClick={() => onConfirm('task')}>
            仅修改这一项
          </button>
          <button type="button" disabled={busy} onClick={() => onConfirm('subtree')}>
            同时修改 {descendantCount} 个子任务
          </button>
          <button type="button" disabled={busy} onClick={onCancel}>取消</button>
        </div>
      </section>
    </div>,
    document.body,
  );
}
