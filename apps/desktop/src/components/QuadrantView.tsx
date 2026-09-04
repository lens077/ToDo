import * as React from 'react';
import type { DesktopSnapshot, DesktopTask, TaskPriority } from '../lib/desktop';
import { dueBands, splitColumns, type DueFilter } from '../lib/goal';
import { TaskCard, type TaskCardActions } from './TaskCard';

interface QuadrantViewProps extends TaskCardActions {
  snapshot: DesktopSnapshot;
  busyKeys: Set<string>;
  readOnly: boolean;
  onSetPriority: (id: string, priority: TaskPriority) => void;
  onRequestPriority: (task: DesktopTask, priority: TaskPriority) => void;
}

const levels = [
  { value: 1 as const, label: '最高' },
  { value: 2 as const, label: '高' },
  { value: 3 as const, label: '中' },
  { value: 4 as const, label: '低' },
];

export function QuadrantView({ snapshot, busyKeys, readOnly, onSetPriority, onRequestPriority, ...actions }: QuadrantViewProps) {
  const [filter, setFilter] = React.useState<DueFilter>('all');
  const [draggedId, setDraggedId] = React.useState<string | null>(null);
  const [announcement, setAnnouncement] = React.useState('');
  const [dropPriority, setDropPriority] = React.useState<TaskPriority | undefined>();
  const columns = React.useMemo(() => splitColumns(snapshot.columns), [snapshot.columns]);
  const bands = React.useMemo(() => dueBands(snapshot), [snapshot]);
  const doneColumns = new Set(columns.done.map((column) => column.id));
  const openTasks = snapshot.tasks.filter((task) => !task.completed && !doneColumns.has(task.column ?? ''));
  const unranked = openTasks.filter((task) => task.priority === null);
  const openIds = new Set(openTasks.flatMap((task) => task.id ? [task.id] : []));
  const passes = (task: DesktopTask) => filter === 'all' || Boolean(task.id && bands[filter].has(task.id));
  const bandCount = (band: Exclude<DueFilter, 'all'>) => [...bands[band]].filter((id) => openIds.has(id)).length;
  const focusId = columns.focus?.id ?? null;
  const order = (items: DesktopTask[]) => [...items].sort((a, b) => {
    const focus = Number(b.column === focusId) - Number(a.column === focusId);
    if (focus) return focus;
    return (a.due ?? '9999-99-99').localeCompare(b.due ?? '9999-99-99');
  });
  const moveTargets = { todo: columns.backlog[0] ?? null, doing: columns.focus, done: columns.done[0] ?? null };
  const dropProps = (priority: TaskPriority) => ({
    onDragOver: (event: React.DragEvent) => {
      if (!draggedId || readOnly) return;
      event.preventDefault();
      setDropPriority(priority);
    },
    onDragLeave: (event: React.DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropPriority(undefined);
    },
    onDrop: (event: React.DragEvent) => {
      event.preventDefault();
      const task = snapshot.tasks.find((item) => item.id === (draggedId ?? event.dataTransfer.getData('text/plain')));
      setDraggedId(null);
      setDropPriority(undefined);
      if (task && task.priority !== priority) {
        setAnnouncement(`正在把“${task.body}”${priority === null ? '移到未定级' : `标为 !${priority}`}`);
        onRequestPriority(task, priority);
      }
    },
  });
  const card = (task: DesktopTask) => (
    <TaskCard
      key={task.id ?? task.body}
      task={task}
      columns={snapshot.columns}
      moveTargets={moveTargets}
      busy={Boolean(task.id && busyKeys.has(`task:${task.id}`))}
      readOnly={readOnly}
      variant="quadrant"
      isInFocus={task.column === focusId}
      {...actions}
      onSetPriority={onSetPriority}
      onDragStart={(id) => setDraggedId(id)}
      onDragEnd={() => { setDraggedId(null); setDropPriority(undefined); }}
    />
  );

  if (snapshot.tasks.length === 0) {
    return <section className="view-empty view-empty--tasks"><p className="view-empty__kicker">TODO.md 已连接</p><h2>还没有 Project Task</h2><p>从顶部快速新增第一项，再把它拖进一个优先级档位。</p></section>;
  }

  return (
    <section className="quadrant-view" aria-labelledby="quadrant-title">
      <header className="view-heading view-heading--quadrant">
        <div className="view-heading__title">
          <h1 id="quadrant-title">优先级象限</h1>
          <p className="view-heading__meta"><span>{openTasks.length} 项未完成</span><span aria-hidden="true">·</span><span>{unranked.length} 项未定级</span>{readOnly && <><span aria-hidden="true">·</span><span>只读浏览</span></>}</p>
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
          const tasks = order(openTasks.filter((task) => task.priority === level.value && passes(task)));
          return <section key={level.value} className={`quadrant-cell quadrant-cell--${level.value}${dropPriority === level.value ? ' is-drop-target' : ''}`} aria-labelledby={`quadrant-${level.value}`} {...dropProps(level.value)}>
            <header><h2 id={`quadrant-${level.value}`}><code>!{level.value}</code> {level.label}</h2><span>{tasks.length}</span></header>
            <div className="quadrant-cell__list">{tasks.length ? tasks.map(card) : <div className="quadrant-empty">{filter === 'all' ? `拖到这里标为 !${level.value}` : '当前筛选下没有任务'}</div>}</div>
          </section>;
        })}
      </div>
      <section className={`unranked-tray${dropPriority === null ? ' is-drop-target' : ''}`} aria-labelledby="unranked-title" {...dropProps(null)}>
        <header><h2 id="unranked-title">未定级 <span>{unranked.length}</span></h2>{unranked.length === 0 && <p>所有未完成任务都已定级</p>}</header>
        {unranked.length > 0 && <div className="unranked-tray__list">{order(unranked.filter(passes)).map(card)}</div>}
      </section>
      <p className="sr-only" aria-live="polite">{announcement}</p>
    </section>
  );
}
