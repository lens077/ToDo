import type { DragEvent } from 'react';
import type { DesktopColumn, DesktopTask } from '../lib/desktop';
import { formatDueDate, shortId } from '../lib/format';
import { Icon } from './Icon';

interface TaskCardProps {
  task: DesktopTask;
  columns: DesktopColumn[];
  busy: boolean;
  readOnly: boolean;
  compact?: boolean;
  onDragStart?: (taskId: string) => void;
  onDragEnd?: () => void;
  onSetCompleted: (taskId: string, completed: boolean) => void;
  onRecordOutcome: (taskId: string, outcome: 'partial' | 'failed') => void;
  onMove: (taskId: string, columnId: string) => void;
}

function localDateKey(): string {
  const date = new Date();
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 10);
}

export function TaskCard({
  task,
  columns,
  busy,
  readOnly,
  compact = false,
  onDragStart,
  onDragEnd,
  onSetCompleted,
  onRecordOutcome,
  onMove,
}: TaskCardProps) {
  const movable = Boolean(task.id && columns.length > 1 && !busy && !readOnly);
  const dueTone = task.due && !task.completed && task.due < localDateKey() ? 'overdue' : 'normal';

  const startDrag = (event: DragEvent<HTMLElement>) => {
    if (!task.id) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', task.id);
    onDragStart?.(task.id);
  };

  return (
    <article
      className={`task-card${task.completed ? ' is-complete' : ''}${busy ? ' is-busy' : ''}${readOnly ? ' is-read-only' : ''}${compact ? ' task-card--compact' : ''}`}
      draggable={movable && !compact}
      aria-disabled={readOnly || undefined}
      onDragStart={startDrag}
      onDragEnd={onDragEnd}
      tabIndex={0}
      aria-label={`${task.completed ? '已完成' : '未完成'} Project Task：${task.body}`}
    >
      <div className="task-card__topline">
        <button
          className="task-check"
          type="button"
          aria-label={task.completed ? `将“${task.body}”标为未完成` : `完成“${task.body}”`}
          aria-pressed={task.completed}
          disabled={readOnly || !task.id || busy}
          onClick={() => task.id && onSetCompleted(task.id, !task.completed)}
        >
          {task.completed && <Icon name="check" />}
        </button>
        <span className="task-id">{shortId(task.id)}</span>
        {!readOnly && task.id && (
          <details className="outcome-menu">
            <summary aria-label={`记录“${task.body}”的任务结局`} title="记录任务结局">
              <span aria-hidden="true">···</span>
            </summary>
            <div className="move-menu__popover" role="menu" aria-label="记录任务结局">
              <span className="move-menu__label">记录结局</span>
              <button type="button" role="menuitem" disabled={busy} onClick={(event) => {
                event.currentTarget.closest('details')?.removeAttribute('open');
                onRecordOutcome(task.id!, 'partial');
              }}>记录部分完成</button>
              <button type="button" role="menuitem" disabled={busy} onClick={(event) => {
                event.currentTarget.closest('details')?.removeAttribute('open');
                onRecordOutcome(task.id!, 'failed');
              }}>记录失败</button>
            </div>
          </details>
        )}
        {!compact && !readOnly && (
          <details className="move-menu">
            <summary aria-label={`移动“${task.body}”到其他列`} title="移动到列">
              <Icon name="move" />
            </summary>
            <div className="move-menu__popover" role="menu" aria-label="移动到列">
              <span className="move-menu__label">移动到</span>
              {columns.map((column) => (
                <button
                  type="button"
                  role="menuitem"
                  key={column.id}
                  disabled={!movable || column.id === task.column}
                  onClick={(event) => {
                    event.currentTarget.closest('details')?.removeAttribute('open');
                    if (task.id) onMove(task.id, column.id);
                  }}
                >
                  <span className={`column-dot column-dot--${column.done ? 'done' : 'active'}`} />
                  {column.name}
                  {column.id === task.column && <span className="move-menu__current">当前</span>}
                </button>
              ))}
            </div>
          </details>
        )}
      </div>

      <p className="task-card__body">{task.body}</p>

      <div className="task-card__metadata">
        {task.priority && (
          <span className={`priority priority--${task.priority}`} aria-label={`优先级 ${task.priority}`}>
            P{task.priority}
          </span>
        )}
        {task.due && (
          <time className={`due-date due-date--${dueTone}`} dateTime={task.due}>
            {formatDueDate(task.due)}
          </time>
        )}
        {task.tags.map((tag) => (
          <span className="task-tag" key={tag}>
            #{tag}
          </span>
        ))}
        {/* Partial state is shown on the card: a 60%-done task with a failed
            attempt must not read as untouched work. */}
        {!task.completed && task.progress !== null && (
          <span className="task-partial" aria-label={`已完成 ${task.progress}%`}>
            ◐ {task.progress}%
          </span>
        )}
        {!task.completed && task.failed_on && (
          <span className="task-failed" aria-label={`上次失败于 ${task.failed_on}`}>
            ⚠ {task.failed_on}
          </span>
        )}
      </div>

      {task.note && !compact && <p className="task-note">↳ {task.note}</p>}

      {busy && <span className="task-card__saving">写入中</span>}
    </article>
  );
}
