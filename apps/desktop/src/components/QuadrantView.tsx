import * as React from 'react';
import type { DesktopSnapshot, TaskPriority } from '../lib/desktop';
import { useDragAutoScroll } from '../lib/dragScroll';
import { formatDueDate } from '../lib/format';
import { dueBands, splitColumns, type DueFilter } from '../lib/goal';
import { groupTasks, quadrantGroups, type QuadrantGroup } from '../lib/quadrant';
import { Icon } from './Icon';
import { TaskCard, type TaskCardActions } from './TaskCard';

interface QuadrantViewProps extends Omit<TaskCardActions, 'onSetPriority'> {
  snapshot: DesktopSnapshot;
  busyKeys: Set<string>;
  readOnly: boolean;
  onSetPriority: (id: string, priority: TaskPriority) => Promise<boolean>;
}

const levels = [
  { value: 1 as const, label: '最高' },
  { value: 2 as const, label: '高' },
  { value: 3 as const, label: '中' },
  { value: 4 as const, label: '低' },
];

export function QuadrantView({ snapshot, busyKeys, readOnly, onSetPriority, ...actions }: QuadrantViewProps) {
  const [filter, setFilter] = React.useState<DueFilter>('all');
  const [draggedId, setDraggedId] = React.useState<string | null>(null);
  const [announcement, setAnnouncement] = React.useState('');
  const [dropPriority, setDropPriority] = React.useState<TaskPriority | undefined>();
  useDragAutoScroll(draggedId !== null);
  const columns = React.useMemo(() => splitColumns(snapshot.columns), [snapshot.columns]);
  const bands = React.useMemo(() => dueBands(snapshot), [snapshot]);
  const groups = React.useMemo(() => quadrantGroups(snapshot), [snapshot]);
  const doneColumns = new Set(columns.done.map((column) => column.id));
  const openTasks = snapshot.tasks.filter((task) => !task.completed && !doneColumns.has(task.column ?? ''));
  const unranked = groups.filter((group) => group.task.priority === null);
  const openIds = new Set(openTasks.flatMap((task) => task.id ? [task.id] : []));
  const passes = (group: QuadrantGroup) => filter === 'all' || groupTasks(group).some((task) => Boolean(task.id && bands[filter].has(task.id)));
  const bandCount = (band: Exclude<DueFilter, 'all'>) => [...bands[band]].filter((id) => openIds.has(id)).length;
  const focusIds = new Set(columns.focusColumns.map((column) => column.id));
  const order = (items: QuadrantGroup[]) => [...items].sort((a, b) => {
    const focus = Number(focusIds.has(b.task.column ?? '')) - Number(focusIds.has(a.task.column ?? ''));
    if (focus) return focus;
    return (a.task.due ?? '9999-99-99').localeCompare(b.task.due ?? '9999-99-99');
  });
  const moveTargets = { todo: columns.backlog[0] ?? null, doing: columns.focus, done: columns.done[0] ?? null };
  const groupBusy = (group: QuadrantGroup) => groupTasks(group).some((task) => task.id && busyKeys.has(`task:${task.id}`));

  React.useEffect(() => {
    if (draggedId && !groups.some((group) => group.task.id === draggedId)) {
      setDraggedId(null);
      setDropPriority(undefined);
      setAnnouncement('任务已在外部更新，本次拖动已取消');
    }
  }, [groups, draggedId]);

  const setGroupPriority = async (group: QuadrantGroup, priority: TaskPriority) => {
    if (!group.task.id || readOnly || group.task.source_writable === false || groupBusy(group)) return;
    const destination = priority === null ? '未定级' : `!${priority}`;
    setAnnouncement(`正在把“${group.task.body}”整组移到${destination}`);
    const written = await onSetPriority(group.task.id, priority);
    setAnnouncement(written ? `“${group.task.body}”整组已移到${destination}` : `“${group.task.body}”整组没有改动`);
  };
  const dropProps = (priority: TaskPriority) => ({
    onDragOver: (event: React.DragEvent) => {
      if (!draggedId || readOnly) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      setDropPriority(priority);
    },
    onDragLeave: (event: React.DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropPriority(undefined);
    },
    onDrop: (event: React.DragEvent) => {
      if (readOnly || !draggedId) return;
      event.preventDefault();
      const group = groups.find((item) => item.task.id === draggedId);
      setDraggedId(null);
      setDropPriority(undefined);
      if (group && groupTasks(group).some((task) => task.priority !== priority)) {
        void setGroupPriority(group, priority);
      }
    },
  });

  const descendants = (nodes: QuadrantGroup[], parentName: string): React.ReactNode => (
    <ul className="quadrant-children" aria-label={`“${parentName}”的子任务`}>
      {nodes.map(({ task, children }) => (
        <li key={task.id ?? `${task.body}:${task.indent_level}`} data-task-id={task.id ?? undefined} draggable={false}>
          <div className={`quadrant-child${task.completed ? ' is-complete' : ''}`}>
            <button
              className="task-check"
              type="button"
              aria-label={task.completed ? `将“${task.body}”标为未完成` : `完成“${task.body}”`}
              aria-pressed={task.completed}
              disabled={readOnly || task.source_writable === false || !task.id || busyKeys.has(`task:${task.id}`)}
              onClick={() => task.id && actions.onSetCompleted(task.id, !task.completed)}
            >
              {task.completed && <Icon name="check" />}
            </button>
            <div className="quadrant-child__content">
              <span className="quadrant-child__body">{task.body}</span>
              <div className="quadrant-child__metadata">
                <span>{task.priority === null ? '未定级' : `!${task.priority}`}</span>
                {task.due && <time dateTime={task.due}>{formatDueDate(task.due)}</time>}
                {task.progress !== null && !task.completed && <span>◐ {task.progress}%</span>}
                {task.failed_on && <span>⚠ {task.failed_on}</span>}
                {task.tags.map((tag) => <span key={tag}>#{tag}</span>)}
              </div>
              {task.note && <p className="task-note">↳ {task.note}</p>}
            </div>
          </div>
          {children.length > 0 && descendants(children, task.body)}
        </li>
      ))}
    </ul>
  );
  const card = (group: QuadrantGroup) => {
    const { task, children } = group;
    const members = groupTasks(group);
    const mixed = members.some((member) => member.priority !== task.priority);
    return (
      <div key={task.id ?? task.body} className="quadrant-task-group" data-group-root={task.id ?? undefined}>
        <TaskCard
          task={task}
          columns={snapshot.columns}
          moveTargets={moveTargets}
          busy={groupBusy(group)}
          readOnly={readOnly}
          variant="quadrant"
          isInFocus={focusIds.has(task.column ?? '')}
          {...actions}
          allowCurrentPriority={mixed}
          onSetPriority={(_id, priority) => void setGroupPriority(group, priority)}
          onDragStart={(id) => setDraggedId(id)}
          onDragEnd={() => { setDraggedId(null); setDropPriority(undefined); }}
        >
          {children.length > 0 && (
            <div className="quadrant-group__children">
              <p className="quadrant-group__summary">{members.length - 1} 个子任务 · 整组改档{mixed && <span> · 组内原优先级不同</span>}</p>
              {descendants(children, task.body)}
            </div>
          )}
        </TaskCard>
      </div>
    );
  };

  if (snapshot.tasks.length === 0) {
    return <section className="view-empty view-empty--tasks"><p className="view-empty__kicker">TODO.md 已连接</p><h2>还没有 Project Task</h2><p>从顶部快速新增第一项，再把它拖进一个优先级档位。</p></section>;
  }

  return (
    <section className="quadrant-view" aria-labelledby="quadrant-title">
      <header className="view-heading view-heading--quadrant">
        <div className="view-heading__title">
          <h1 id="quadrant-title">优先级象限</h1>
          <p className="view-heading__meta"><span>{groups.length} 个任务组</span><span aria-hidden="true">·</span><span>{openTasks.length} 项未完成</span><span aria-hidden="true">·</span><span>{unranked.length} 组未定级</span>{readOnly && <><span aria-hidden="true">·</span><span>只读浏览</span></>}</p>
        </div>
        <div className="due-summary" role="group" aria-label="按截止日期筛选象限">
          {([['all', '全部', openTasks.length], ['overdue', '逾期', bandCount('overdue')], ['today', '今天', bandCount('today')], ['upcoming', '7 天内', bandCount('upcoming')]] as Array<[DueFilter, string, number]>).map(([value, label, count]) => (
            <button type="button" key={value} className={`due-summary__chip due-summary__chip--${value}${filter === value ? ' is-active' : ''}`} aria-pressed={filter === value} disabled={value !== 'all' && count === 0} onClick={() => setFilter(value)}><span>{label}</span><strong>{count}</strong></button>
          ))}
        </div>
      </header>
      {openTasks.length === 0 && <div className="quadrant-all-done">未完成的工作都清空了，已完成在「目标」页</div>}
      <div className="quadrant-grid">
        {levels.map((level) => {
          const items = order(groups.filter((group) => group.task.priority === level.value && passes(group)));
          return <section key={level.value} className={`quadrant-cell quadrant-cell--${level.value}${dropPriority === level.value ? ' is-drop-target' : ''}`} aria-labelledby={`quadrant-${level.value}`} {...dropProps(level.value)}>
            <header><h2 id={`quadrant-${level.value}`}><code>!{level.value}</code> {level.label}</h2><span>{items.length} 组</span></header>
            <div className="quadrant-cell__list">{items.length ? items.map(card) : <div className="quadrant-empty">{filter === 'all' ? `拖到这里标为 !${level.value}` : '当前筛选下没有任务'}</div>}</div>
          </section>;
        })}
      </div>
      <section className={`unranked-tray${dropPriority === null ? ' is-drop-target' : ''}`} aria-labelledby="unranked-title" {...dropProps(null)}>
        <header><h2 id="unranked-title">未定级 <span>{unranked.length}</span></h2>{unranked.length === 0 && <p>所有任务组都已定级</p>}</header>
        {unranked.length > 0 && <div className="unranked-tray__list">{order(unranked.filter(passes)).map(card)}</div>}
      </section>
      <p className="sr-only" aria-live="polite">{announcement}</p>
    </section>
  );
}
