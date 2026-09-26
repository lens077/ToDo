import * as React from 'react';
import type { DesktopColumn, DesktopSnapshot, DesktopTask, TaskPriority } from '../lib/desktop';
import {
  clampFocusLimit,
  dueBands,
  FOCUS_LIMIT_MAX,
  FOCUS_LIMIT_MIN,
  readGoalLayout,
  splitColumns,
  writeGoalLayout,
  type DueFilter,
  type GoalLayoutState,
} from '../lib/goal';
import { useAppSettings } from '../lib/appSettings';
import { Icon } from './Icon';
import { Ribbon } from './Ribbon';
import { TaskCard, type TaskCardActions } from './TaskCard';

interface GoalViewProps extends Omit<TaskCardActions, 'onSetCompleted' | 'onMove'> {
  snapshot: DesktopSnapshot;
  /** Turns the loose list reader (Beta) on for a source with no checkboxes. */
  onSetLoose?: (path: string, loose: boolean) => void;
  busyKeys: Set<string>;
  readOnly: boolean;
  onSetCompleted: (id: string, completed: boolean) => Promise<boolean>;
  onMove: (id: string, column: string) => Promise<boolean>;
  onSetPriority: (id: string, priority: TaskPriority) => void;
}

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

function useGoalLayout(): [GoalLayoutState, (patch: Partial<GoalLayoutState>) => void] {
  const [state, setState] = React.useState<GoalLayoutState>(() => readGoalLayout(storage()));
  const update = React.useCallback((patch: Partial<GoalLayoutState>) => {
    setState((current) => {
      const next = { ...current, ...patch, focusLimit: clampFocusLimit(patch.focusLimit ?? current.focusLimit) };
      writeGoalLayout(storage(), next);
      return next;
    });
  }, []);
  return [state, update];
}

/**
 * Drop-target bookkeeping shared by the three regions. A region accepts a
 * drop for one column id; the move is an intent, and the card only lands once
 * the file write succeeds (the snapshot then re-renders it in place).
 */
function useDropTargets(
  snapshot: DesktopSnapshot,
  readOnly: boolean,
  onMove: GoalViewProps['onMove'],
  onConfigureColumns: GoalViewProps['onConfigureColumns'],
) {
  type RegionRole = 'focus' | 'backlog' | 'done';
  const [draggedId, setDraggedId] = React.useState<string | null>(null);
  const [dropTarget, setDropTarget] = React.useState<{ column: string | null; role?: RegionRole } | null>(null);
  const [announcement, setAnnouncement] = React.useState('');

  const moveTask = React.useCallback(
    async (taskId: string, columnId: string, allowSameColumn = false) => {
      if (readOnly) return;
      const task = snapshot.tasks.find((item) => item.id === taskId);
      const destination = snapshot.columns.find((column) => column.id === columnId);
      if (!task || !destination) return;
      if (
        !allowSameColumn
        && task.column === columnId
        && task.completed === destination.done
      ) {
        setAnnouncement(`“${task.body}”仍在${destination.name}`);
        return;
      }
      setAnnouncement(`正在把“${task.body}”移动到${destination.name}顶部`);
      const succeeded = await onMove(taskId, columnId);
      setAnnouncement(
        succeeded
          ? `“${task.body}”已移动到${destination.name}顶部`
          : `“${task.body}”没有移动，仍保留原位置`,
      );
      requestAnimationFrame(() => {
        const card = Array.from(document.querySelectorAll<HTMLElement>('[data-task-id]'))
          .find((element) => element.dataset.taskId === taskId);
        card?.focus();
      });
    },
    [onMove, readOnly, snapshot.columns, snapshot.tasks],
  );

  const writableTask = (taskId: string | null) => {
    const task = snapshot.tasks.find((item) => item.id === taskId);
    if (readOnly || !task?.id || task.source_writable === false) return null;
    if (snapshot.sources && !snapshot.sources.some((source) => source.path === task.source_path && source.status === 'managed' && source.writable)) return null;
    return task;
  };
  const resolveColumn = (id: string | null, taskId: string | null, role?: RegionRole) => {
    const task = writableTask(taskId);
    if (!task) return null;
    if (!role || !snapshot.sources) return id;
    const own = splitColumns(snapshot.columns.filter((column) => column.source_path === task.source_path));
    return (role === 'focus' ? own.focus : own[role][0])?.id ?? null;
  };
  const canCreateAndStart = (taskId: string | null, role?: RegionRole) => {
    const task = writableTask(taskId);
    return Boolean(role === 'focus' && onConfigureColumns && task?.source_path !== undefined
      && task?.local_id && task.column === null && !task.completed
      && !snapshot.columns.some((column) => column.source_path === task.source_path));
  };
  const dropProps = (id: string | null, role?: RegionRole) => ({
    onDragOver: (event: React.DragEvent) => {
      const columnId = resolveColumn(id, draggedId, role);
      if (!draggedId || (!columnId && !canCreateAndStart(draggedId, role))) {
        event.dataTransfer.dropEffect = 'none';
        return;
      }
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      setDropTarget({ column: columnId, role });
    },
    onDragLeave: (event: React.DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropTarget(null);
    },
    onDrop: (event: React.DragEvent) => {
      event.preventDefault();
      const taskId = draggedId ?? event.dataTransfer.getData('text/plain');
      const columnId = resolveColumn(id, taskId, role);
      const task = writableTask(taskId);
      setDraggedId(null);
      setDropTarget(null);
      if (taskId && columnId) void moveTask(taskId, columnId);
      else if (task && canCreateAndStart(taskId, role)) {
        const opener = Array.from(document.querySelectorAll<HTMLElement>('[data-task-id]')).find((element) => element.dataset.taskId === taskId) ?? null;
        setAnnouncement('请预览创建默认列并推进此任务的差异；确认前不写入。');
        onConfigureColumns?.(task, opener, true);
      }
    },
  });

  const cardDragProps = {
    onDragStart: (id: string) => setDraggedId(id),
    onDragEnd: () => {
      setDraggedId(null);
      setDropTarget(null);
    },
  };

  const isDropTarget = (columnId: string | null, role?: RegionRole) => Boolean(draggedId && dropTarget
    && (role ? dropTarget.role === role : columnId && dropTarget.column === columnId));

  return { moveTask, dropProps, cardDragProps, isDropTarget, announcement };
}

function CollapseButton({
  collapsed,
  axis,
  label,
  controls,
  onToggle,
}: {
  collapsed: boolean;
  axis: 'right' | 'bottom';
  label: string;
  controls: string;
  onToggle: () => void;
}) {
  const icon = axis === 'right'
    ? (collapsed ? 'panel-right-collapsed' : 'panel-right')
    : (collapsed ? 'panel-bottom-collapsed' : 'panel-bottom');
  const text = collapsed ? `展开${label}` : `收起${label}`;
  return (
    <button
      type="button"
      className="region-toggle"
      aria-expanded={!collapsed}
      aria-controls={controls}
      aria-label={text}
      title={text}
      onClick={onToggle}
    >
      <Icon name={icon} />
    </button>
  );
}

export function GoalView({
  snapshot,
  busyKeys,
  readOnly,
  onSetCompleted,
  onSetPartial,
  onSetFailed,
  onMove,
  onSetPriority,
  onSetLoose,
  onConfigureColumns,
}: GoalViewProps) {
  const [layout, updateLayout] = useGoalLayout();
  const [{ density }] = useAppSettings();
  const [dueFilter, setDueFilter] = React.useState<DueFilter>('all');
  const columns = React.useMemo(() => splitColumns(snapshot.columns), [snapshot.columns]);
  const bands = React.useMemo(() => dueBands(snapshot), [snapshot]);
  const drop = useDropTargets(snapshot, readOnly, onMove, onConfigureColumns);

  const tasksIn = (column: DesktopColumn | null) =>
    column ? snapshot.tasks.filter((task) => task.column === column.id) : [];
  // Finished work is read by state, not by column: a task ticked off while it
  // still sits in an active column belongs in the finished rail (where it can
  // be un-ticked or dragged into the done column), not hidden in the queue.
  const isDoneColumn = (id: string | null) => columns.done.some((column) => column.id === id);
  const open = (tasks: DesktopTask[]) => tasks.filter((task) => !task.completed && !isDoneColumn(task.column));
  const focusTasks = open(snapshot.tasks.filter((task) => columns.focusColumns.some((column) => column.id === task.column)));
  const orphanTasks = open(snapshot.tasks.filter((task) => task.column === null));
  const doneTasks = snapshot.tasks.filter((task) => task.completed || isDoneColumn(task.column));
  const backlogTotal = columns.backlog.reduce((sum, column) => sum + open(tasksIn(column)).length, 0) + orphanTasks.length;
  const columnName = (id: string | null) => snapshot.columns.find((column) => column.id === id)?.name ?? null;

  const spotlight = focusTasks.slice(0, layout.focusLimit);
  const queued = focusTasks.slice(layout.focusLimit);

  const passesFilter = (task: DesktopTask) =>
    dueFilter === 'all' || (task.id !== null && bands[dueFilter].has(task.id));
  const filterCounts = {
    overdue: bands.overdue.size,
    today: bands.today.size,
    upcoming: bands.upcoming.size,
  };

  const moveTargets = {
    todo: columns.backlog[0] ?? null,
    doing: columns.focus,
    done: columns.done[0] ?? null,
  };
  const cardActions = (task: DesktopTask, variant: 'card' | 'compact' | 'focus') => ({
    task,
    columns: snapshot.columns,
    moveTargets,
    busy: Boolean(task.id && busyKeys.has(`task:${task.id}`)),
    readOnly,
    variant,
    onSetCompleted: (id: string, completed: boolean) => void onSetCompleted(id, completed),
    onSetPartial,
    onSetFailed,
    onMove: (id: string, column: string) => void drop.moveTask(id, column),
    onSetPriority,
    onConfigureColumns,
  });

  if (snapshot.tasks.length === 0) {
    // A managed file with no checkbox lines is read correctly and simply has
    // nothing to show; say so, rather than implying the connection failed.
    // In a single-source view the snapshot still lists every discovered file;
    // the explanation must be about the file the person is looking at.
    const managedSources = (snapshot.sources ?? [])
      .filter((source) => source.status === 'managed')
      .filter((source) => snapshot.selected_source == null || source.path === snapshot.selected_source);
    const onlyIndexFiles = managedSources.length > 0 && managedSources.every((source) => source.task_count === 0);
    const looseCandidates = onSetLoose ? managedSources.filter((source) => (source.loose_candidates ?? 0) > 0) : [];
    return (
      <section className="view-empty view-empty--tasks" aria-labelledby="empty-goal-title">
        <h2 id="empty-goal-title">还没有 Project Task<span>{snapshot.sources ? snapshot.sources.length === 0 ? '项目目录已打开' : onlyIndexFiles ? '文件已读取，其中没有复选框' : '来源已连接' : 'TODO.md 已连接'}</span></h2>
        <p>{snapshot.sources?.length === 0
          ? '未发现 TODO.md / TODO / TODO.txt，也没有 frontmatter 声明 todo-spec 的 Markdown。请放入任务文件后刷新；不会自动创建文件。'
          : onlyIndexFiles
            ? `${managedSources.map((source) => source.path).join('、')} 已接管，但任务只从「- [ ]」复选框行读取，这个文件里一行也没有。若清单拆在其他 Markdown 里，给那些文件加上 frontmatter「todo-spec: 1」即可被发现并录入；或从顶部新增第一项。`
            : snapshot.sources
              ? '请选择可写来源后新增第一项。未接管的清单需先预览接管差异。'
              : '从顶部快速新增第一项。它会写回当前项目，而不是留在界面缓存里。'}</p>
        {onlyIndexFiles && looseCandidates.length > 0 && (
          <div className="loose-offer" role="note">
            <div className="loose-offer__copy">
              <strong>不过{looseCandidates.length === 1 ? `「${looseCandidates[0].path}」` : `这 ${looseCandidates.length} 个文件`}里有「- 」列表项（{looseCandidates.reduce((sum, source) => sum + (source.loose_candidates ?? 0), 0)} 行）。</strong>
              <span>试试宽松解析（Beta）：把列表项当作任务显示，只读、不改文件，随时可以关闭。普通 Markdown 的列表并不都是任务，请自行判断。</span>
            </div>
            <div className="loose-offer__actions">
              {looseCandidates.map((source) => (
                <button type="button" key={source.path} className="line-action" onClick={() => onSetLoose?.(source.path, true)}>
                  {looseCandidates.length === 1 ? '启用宽松解析' : `启用：${source.path}`}
                </button>
              ))}
            </div>
          </div>
        )}
      </section>
    );
  }

  const layoutClass = [
    'goal-layout',
    layout.todoCollapsed ? 'is-todo-collapsed' : '',
    layout.doneCollapsed ? 'is-done-collapsed' : '',
    columns.done.length === 0 && doneTasks.length === 0 ? 'has-no-done' : '',
  ].filter(Boolean).join(' ');

  const focusName = columns.focusColumns.length > 1 ? '推进中' : columns.focus?.name ?? '推进中';
  const doneName = columns.done.length === 1 ? columns.done[0].name : '已完成';

  return (
    <section className="goal-view" aria-labelledby="goal-title">
      <header className="view-heading view-heading--goal">
        {density !== 'efficient' && <Ribbon className="ribbon--heading" />}
        <div className="view-heading__title">
          <h1 id="goal-title">目标</h1>
          <p className="view-heading__meta">
            <span>{snapshot.tasks.length} 项 Project Task</span>
            <span aria-hidden="true">·</span>
            <span>{focusTasks.length} 项推进中</span>
            {readOnly && (
              <>
                <span aria-hidden="true">·</span>
                <span>只读浏览</span>
              </>
            )}
          </p>
        </div>
        <div className="due-summary" role="group" aria-label="按截止日期筛选队列">
          {([
            ['all', '全部', backlogTotal + focusTasks.length],
            ['overdue', '逾期', filterCounts.overdue],
            ['today', '今天', filterCounts.today],
            ['upcoming', '7 天内', filterCounts.upcoming],
          ] as Array<[DueFilter, string, number]>).map(([value, label, count]) => (
            <button
              type="button"
              key={value}
              className={`due-summary__chip due-summary__chip--${value}${dueFilter === value ? ' is-active' : ''}`}
              aria-pressed={dueFilter === value}
              disabled={value !== 'all' && count === 0}
              onClick={() => setDueFilter(value)}
            >
              <span>{label}</span>
              <strong>{count}</strong>
            </button>
          ))}
        </div>
      </header>

      <div className={layoutClass}>
        {/* ── In flight ─────────────────────────────────────────────── */}
        <section
          className={`goal-region goal-region--doing${drop.isDropTarget(columns.focus?.id ?? null, 'focus') ? ' is-drop-target' : ''}`}
          aria-labelledby="goal-doing-title"
          {...drop.dropProps(columns.focus?.id ?? null, 'focus')}
        >
          <header className="goal-region__header">
            <div className="goal-region__title">
              <h2 id="goal-doing-title">{focusName}</h2>
              <span className="goal-region__count">{focusTasks.length}</span>
            </div>
            <div className="focus-limit" role="group" aria-label="同时显示的推进中任务数">
              <span className="focus-limit__label">同时显示</span>
              <button
                type="button"
                aria-label="减少同时显示数"
                disabled={layout.focusLimit <= FOCUS_LIMIT_MIN}
                onClick={() => updateLayout({ focusLimit: layout.focusLimit - 1 })}
              >
                <Icon name="minus" />
              </button>
              <output aria-live="polite">{layout.focusLimit}</output>
              <button
                type="button"
                aria-label="增加同时显示数"
                disabled={layout.focusLimit >= FOCUS_LIMIT_MAX}
                onClick={() => updateLayout({ focusLimit: layout.focusLimit + 1 })}
              >
                <Icon name="plus" />
              </button>
            </div>
          </header>

          <div className="goal-region__body">
            {spotlight.length > 0 ? (
              <div className="focus-grid" data-count={spotlight.length}>
                {spotlight.map((task) => (
                  <TaskCard key={task.id ?? task.body} {...cardActions(task, 'focus')} {...drop.cardDragProps} />
                ))}
              </div>
            ) : (
              <div className="focus-empty">
                <strong>还没有推进中的任务</strong>
                <p>
                  {readOnly ? '当前只读，不能开始处理或移动任务。'
                    : columns.focus
                      ? '把一项工作拖到这里，按它所属来源的推进列开始处理；无列来源会先预览创建列并推进任务。'
                      : snapshot.columns.length === 0
                        ? '把一项未归列任务拖到这里，或在任务菜单选择「开始处理」。先预览创建默认列并推进此任务，确认后才写入。'
                        : '当前来源没有可用的推进列；不会覆盖已有列或借用其他来源的列。'}
                </p>
              </div>
            )}

            {queued.length > 0 && (
              <div className="focus-queue">
                <span className="focus-queue__label">「{focusName}」还有 {queued.length} 项未展开</span>
                <ul>
                  {queued.map((task) => (
                    <li key={task.id ?? task.body}>
                      <span className="focus-queue__body">{task.body}{task.source_path && <small className="task-source"> · {task.source_path}</small>}</span>
                      {task.progress !== null && <span className="task-partial">◐ {task.progress}%</span>}
                      {!readOnly && task.source_writable !== false && task.id && columns.focus && (
                        <button
                          type="button"
                          className="focus-queue__promote"
                          disabled={busyKeys.has(`task:${task.id}`)}
                          onClick={() => { const own = columns.focusColumns.find((column) => column.source_path === task.source_path); if (own) void drop.moveTask(task.id!, own.id, true); }}
                        >
                          提到最前
                        </button>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            )}

          </div>
        </section>

        {/* ── Queue ─────────────────────────────────────────────────── */}
        <section
          id="goal-todo-region"
          className={`goal-region goal-region--todo${layout.todoCollapsed ? ' is-collapsed' : ''}${layout.todoCollapsed && drop.isDropTarget(columns.backlog[0]?.id ?? null, 'backlog') ? ' is-drop-target' : ''}`}
          aria-labelledby="goal-todo-title"
          {...(layout.todoCollapsed ? drop.dropProps(columns.backlog[0]?.id ?? null, 'backlog') : {})}
        >
          <header className="goal-region__header">
            <div className="goal-region__title">
              <h2 id="goal-todo-title">
                {columns.backlog.length === 1 ? columns.backlog[0].name : '待办'}
              </h2>
              <span className="goal-region__count">{backlogTotal}</span>
              {dueFilter !== 'all' && !layout.todoCollapsed && (
                <span className="goal-region__filter">
                  筛选：{dueFilter === 'overdue' ? '逾期' : dueFilter === 'today' ? '今天' : '7 天内'}
                </span>
              )}
            </div>
            <CollapseButton
              collapsed={layout.todoCollapsed}
              axis="bottom"
              label="待办"
              controls="goal-todo-region"
              onToggle={() => updateLayout({ todoCollapsed: !layout.todoCollapsed })}
            />
          </header>

          {!layout.todoCollapsed && (
            <div className={`goal-region__body todo-lanes${columns.backlog.length + (orphanTasks.length ? 1 : 0) > 1 ? ' todo-lanes--multi' : ''}`}>
              {columns.backlog.map((column) => {
                const tasks = open(tasksIn(column)).filter(passesFilter);
                const total = open(tasksIn(column)).length;
                return (
                  <section
                    key={column.id}
                    className={`todo-lane${drop.isDropTarget(column.id) ? ' is-drop-target' : ''}`}
                    aria-label={column.name}
                    {...drop.dropProps(column.id)}
                  >
                    {columns.backlog.length > 1 && (
                      <header className="todo-lane__header">
                        <h3>{column.name}{column.source_path && <small className="task-source"> · {column.source_path}</small>}</h3>
                        <span>{total}</span>
                      </header>
                    )}
                    <div className="todo-lane__cards">
                      {tasks.length > 0 ? (
                        tasks.map((task, index) => (
                          <div className="task-card-wrap" style={{ '--task-index': index } as React.CSSProperties} key={task.id ?? `${column.id}-${index}`}>
                            <TaskCard {...cardActions(task, 'card')} {...drop.cardDragProps} />
                          </div>
                        ))
                      ) : (
                        <div className="column-empty">
                          <span />
                          <p>{total > 0 ? '当前筛选下没有任务。' : '把 Project Task 拖到这里'}</p>
                        </div>
                      )}
                    </div>
                  </section>
                );
              })}
              {orphanTasks.length > 0 && (
                <section className="todo-lane todo-lane--orphan" aria-label="未归列">
                  <header className="todo-lane__header">
                    <h3>未归列</h3>
                    <span>{orphanTasks.length}</span>
                  </header>
                  <div className="todo-lane__cards">
                    {orphanTasks.filter(passesFilter).map((task, index) => (
                      <div className="task-card-wrap" style={{ '--task-index': index } as React.CSSProperties} key={task.id ?? `orphan-${index}`}>
                        <TaskCard {...cardActions(task, 'card')} {...drop.cardDragProps} />
                      </div>
                    ))}
                  </div>
                </section>
              )}
              {columns.backlog.length === 0 && orphanTasks.length === 0 && (
                <div className="column-empty">
                  <span />
                  <p>没有待办列；所有活动工作都在 {focusName}。</p>
                </div>
              )}
            </div>
          )}
        </section>

        {/* ── Finished ──────────────────────────────────────────────── */}
        {(columns.done.length > 0 || doneTasks.length > 0) && (
          <aside
            id="goal-done-region"
            className={`goal-region goal-region--done${layout.doneCollapsed ? ' is-collapsed' : ''}${drop.isDropTarget(columns.done[0]?.id ?? null, 'done') ? ' is-drop-target' : ''}`}
            aria-labelledby="goal-done-title"
            {...drop.dropProps(columns.done[0]?.id ?? null, 'done')}
          >
            <header className="goal-region__header">
              <div className="goal-region__title">
                <Icon name="check" className="goal-region__done-icon" />
                <h2 id="goal-done-title">{doneName}</h2>
                <span className="goal-region__count">{doneTasks.length}</span>
              </div>
              <CollapseButton
                collapsed={layout.doneCollapsed}
                axis="right"
                label={doneName}
                controls="goal-done-region"
                onToggle={() => updateLayout({ doneCollapsed: !layout.doneCollapsed })}
              />
            </header>
            {!layout.doneCollapsed && (
              <div className="goal-region__body done-list">
                {doneTasks.length > 0 ? (
                  doneTasks.map((task, index) => (
                    <div className="task-card-wrap" style={{ '--task-index': index } as React.CSSProperties} key={task.id ?? `done-${index}`}>
                      {columns.done.length > 0 && !isDoneColumn(task.column) && (
                        <span className="done-list__stray">仍在「{columnName(task.column) ?? '未归列'}」，拖动或打开菜单归档到「{doneName}」</span>
                      )}
                      <TaskCard {...cardActions(task, 'compact')} {...drop.cardDragProps} />
                    </div>
                  ))
                ) : (
                  <div className="column-empty">
                    <span />
                    <p>完成的工作会归档在这里</p>
                  </div>
                )}
              </div>
            )}
          </aside>
        )}
      </div>

      <p className="sr-only" aria-live="polite">
        {drop.announcement}
      </p>
    </section>
  );
}
