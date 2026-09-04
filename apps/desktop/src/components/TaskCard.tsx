import * as React from 'react';
import type { DragEvent } from 'react';
import type { DesktopColumn, DesktopTask } from '../lib/desktop';
import { formatDueDate, shortId } from '../lib/format';
import { Icon } from './Icon';
import { TaskContextMenu, type TaskMoveTargets } from './TaskContextMenu';

export type TaskCardVariant = 'card' | 'compact' | 'focus';

export interface TaskCardActions {
  onSetCompleted: (taskId: string, completed: boolean) => void;
  /** Writes a `◐` partial outcome. Resolves `true` when the file accepted it. */
  onSetPartial: (taskId: string, percent: number, note: string) => Promise<boolean>;
  /** Writes a `⚠` failed attempt. Resolves `true` when the file accepted it. */
  onSetFailed: (taskId: string, note: string) => Promise<boolean>;
  onMove: (taskId: string, columnId: string) => void;
}

interface TaskCardProps extends TaskCardActions {
  task: DesktopTask;
  columns: DesktopColumn[];
  moveTargets: TaskMoveTargets;
  busy: boolean;
  readOnly: boolean;
  variant?: TaskCardVariant;
  onDragStart?: (taskId: string) => void;
  onDragEnd?: () => void;
}

type InlineOutcome = 'partial' | 'failed';

function localDateKey(): string {
  const date = new Date();
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 10);
}

/**
 * Records a partial or failed outcome inside the card.
 *
 * Writing "where I got to" is part of the work, not an interruption of it, so
 * the note is typed in place: no modal, no focus trap, Escape simply closes it.
 * The card keeps its position and the rest of the board stays readable.
 */
function OutcomeInline({
  task,
  outcome,
  busy,
  onCancel,
  onSetPartial,
  onSetFailed,
}: {
  task: DesktopTask;
  outcome: InlineOutcome;
  busy: boolean;
  onCancel: () => void;
  onSetPartial: TaskCardActions['onSetPartial'];
  onSetFailed: TaskCardActions['onSetFailed'];
}) {
  const [percent, setPercent] = React.useState(task.progress ?? 50);
  const [note, setNote] = React.useState('');
  const trimmed = note.trim();
  const percentValid = percent >= 1 && percent <= 99;
  const canSubmit = Boolean(trimmed) && (outcome !== 'partial' || percentValid) && !busy;
  const noteLabel = outcome === 'partial' ? '进度说明' : '失败原因';

  const submit = async () => {
    if (!canSubmit || !task.id) return;
    const written = outcome === 'partial'
      ? await onSetPartial(task.id, percent, trimmed)
      : await onSetFailed(task.id, trimmed);
    if (written) onCancel();
  };

  return (
    <form
      className={`outcome-inline outcome-inline--${outcome}`}
      aria-label={outcome === 'partial' ? `记录“${task.body}”的部分完成` : `记录“${task.body}”的失败`}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.stopPropagation();
          onCancel();
        }
      }}
    >
      {outcome === 'partial' && (
        <div className="outcome-inline__percent">
          <input
            type="range"
            min={1}
            max={99}
            step={1}
            value={percent}
            aria-label="完成比例"
            onChange={(event) => setPercent(Number(event.target.value))}
          />
          <label className="outcome-inline__percent-field">
            <input
              type="number"
              min={1}
              max={99}
              inputMode="numeric"
              value={percent}
              aria-label="完成比例数值"
              onChange={(event) => setPercent(Number(event.target.value))}
            />
            <span>%</span>
          </label>
        </div>
      )}
      <div className="outcome-inline__note">
        <input
          type="text"
          autoFocus
          value={note}
          aria-label={noteLabel}
          placeholder={outcome === 'partial' ? '做到哪了（回车记录）' : '失败原因（回车记录）'}
          onChange={(event) => setNote(event.target.value)}
        />
        <button type="submit" className="outcome-inline__submit" disabled={!canSubmit}>
          {busy ? '写入中' : '记录'}
        </button>
        <button type="button" className="outcome-inline__cancel" aria-label="取消记录" onClick={onCancel} disabled={busy}>
          <Icon name="close" />
        </button>
      </div>
      <small className="outcome-inline__hint">
        {outcome === 'partial'
          ? '写入 ◐ 与 ↳ 说明；任务保留在当前列。'
          : '写入 ⚠ 与 ↳ 说明；任务不会被标为完成。'}
      </small>
    </form>
  );
}

export function TaskCard({
  task,
  columns,
  moveTargets,
  busy,
  readOnly,
  variant = 'card',
  onDragStart,
  onDragEnd,
  onSetCompleted,
  onSetPartial,
  onSetFailed,
  onMove,
}: TaskCardProps) {
  const [inline, setInline] = React.useState<InlineOutcome | null>(null);
  const [contextMenu, setContextMenu] = React.useState<{
    x: number;
    y: number;
    restoreFocus: HTMLElement | null;
  } | null>(null);
  const focus = variant === 'focus';
  const movable = Boolean(task.id && columns.length > 1 && !busy && !readOnly);
  const dueTone = task.due && !task.completed && task.due < localDateKey() ? 'overdue' : 'normal';
  const canRecord = Boolean(task.id) && !readOnly && !task.completed;

  React.useEffect(() => {
    if (task.completed) setInline(null);
  }, [task.completed]);

  const startDrag = (event: DragEvent<HTMLElement>) => {
    if (!task.id) return;
    setContextMenu(null);
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', task.id);
    onDragStart?.(task.id);
  };

  const openMenuAt = (x: number, y: number, restoreFocus: HTMLElement | null) => {
    if (movable && !inline) setContextMenu({ x, y, restoreFocus });
  };
  const closeMenu = (restoreFocus = false) => {
    const target = contextMenu?.restoreFocus;
    setContextMenu(null);
    if (restoreFocus && target) {
      requestAnimationFrame(() => {
        if (target.isConnected) target.focus();
      });
    }
  };

  const moveMenu = !readOnly && task.id && (
    <button
      type="button"
      className="move-menu"
      aria-label={`打开“${task.body}”的任务菜单`}
      aria-haspopup="menu"
      aria-expanded={Boolean(contextMenu)}
      title="任务菜单"
      disabled={!movable}
      onClick={(event) => {
        const rect = event.currentTarget.getBoundingClientRect();
        openMenuAt(rect.right + 4, rect.bottom + 4, event.currentTarget);
      }}
    >
      <Icon name="move" />
      {focus && <span>移动</span>}
    </button>
  );

  const partialButton = canRecord && (
    <button
      type="button"
      className={`outcome-button outcome-button--partial${inline === 'partial' ? ' is-open' : ''}`}
      aria-label={`部分完成“${task.body}”`}
      aria-expanded={inline === 'partial'}
      title="记录部分完成"
      disabled={busy}
      onClick={() => setInline((current) => (current === 'partial' ? null : 'partial'))}
    >
      <Icon name="half" />
      {focus && <span>部分完成</span>}
    </button>
  );

  const failedButton = canRecord && (
    <button
      type="button"
      className={`outcome-button outcome-button--failed${inline === 'failed' ? ' is-open' : ''}`}
      aria-label={`记录“${task.body}”的失败`}
      aria-expanded={inline === 'failed'}
      title="记录失败"
      disabled={busy}
      onClick={() => setInline((current) => (current === 'failed' ? null : 'failed'))}
    >
      <Icon name="alert" />
      {focus && <span>记录失败</span>}
    </button>
  );

  return (
    <article
      className={`task-card task-card--${variant}${task.completed ? ' is-complete' : ''}${busy ? ' is-busy' : ''}${readOnly ? ' is-read-only' : ''}${inline ? ' is-editing' : ''}`}
      data-task-id={task.id ?? undefined}
      draggable={movable && !inline}
      aria-disabled={readOnly || undefined}
      onDragStart={startDrag}
      onDragEnd={onDragEnd}
      onContextMenu={(event) => {
        if (!movable || inline) return;
        event.preventDefault();
        openMenuAt(event.clientX, event.clientY, event.currentTarget);
      }}
      onKeyDown={(event) => {
        if (!movable || inline) return;
        if ((event.shiftKey && event.key === 'F10') || event.key === 'ContextMenu') {
          event.preventDefault();
          const rect = event.currentTarget.getBoundingClientRect();
          openMenuAt(
            rect.left + Math.min(48, rect.width / 2),
            rect.top + 32,
            document.activeElement instanceof HTMLElement ? document.activeElement : event.currentTarget,
          );
        }
      }}
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
        {focus && task.progress !== null && !task.completed && (
          <span className="task-card__progress-label" aria-label={`已完成 ${task.progress}%`}>◐ {task.progress}%</span>
        )}
        {!focus && (
          <div className="task-card__tools">
            {partialButton}
            {failedButton}
            {moveMenu}
          </div>
        )}
      </div>

      <p className="task-card__body">{task.body}</p>

      {focus && task.progress !== null && !task.completed && (
        <div className="task-card__progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={task.progress} aria-label="完成比例">
          <span style={{ width: `${task.progress}%` }} />
        </div>
      )}

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
        {!focus && !task.completed && task.progress !== null && (
          <span className="task-partial" aria-label={`已完成 ${task.progress}%`}>
            ◐ {task.progress}%
          </span>
        )}
        {!task.completed && task.failed_on && (
          <span className="task-failed" aria-label={`上次失败于 ${task.failed_on}`}>
            ⚠ {task.failed_on}
          </span>
        )}
        {focus && task.evidence && (
          <code className="task-evidence" title="Git 物证区间">🔗 {task.evidence.slice(0, 7)}…</code>
        )}
      </div>

      {task.note && <p className="task-note">↳ {task.note}</p>}

      {focus && (
        <div className="task-card__actions">
          {partialButton}
          {failedButton}
          {moveMenu}
        </div>
      )}

      {inline && task.id && (
        <OutcomeInline
          key={inline}
          task={task}
          outcome={inline}
          busy={busy}
          onCancel={() => setInline(null)}
          onSetPartial={onSetPartial}
          onSetFailed={onSetFailed}
        />
      )}

      {contextMenu && task.id && (
        <TaskContextMenu
          task={task}
          columns={columns}
          targets={moveTargets}
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={closeMenu}
          onMove={(columnId) => onMove(task.id!, columnId)}
        />
      )}

      {busy && <span className="task-card__saving">写入中</span>}
    </article>
  );
}
