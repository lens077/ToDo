import type { DesktopSnapshot, DesktopTask } from '../lib/desktop';
import { TaskCard } from './TaskCard';

interface TodayViewProps {
  snapshot: DesktopSnapshot;
  busyKeys: Set<string>;
  onSetCompleted: (id: string, completed: boolean) => Promise<boolean>;
  onMove: (id: string, column: string) => Promise<boolean>;
}

interface TodayGroupProps extends TodayViewProps {
  index: string;
  title: string;
  description: string;
  tasks: DesktopTask[];
  tone: 'overdue' | 'today' | 'upcoming' | 'next';
}

function TodayGroup({
  index,
  title,
  description,
  tasks,
  tone,
  snapshot,
  busyKeys,
  onSetCompleted,
  onMove,
}: TodayGroupProps) {
  return (
    <section className={`today-group today-group--${tone}`} aria-labelledby={`today-${tone}`}>
      <header className="today-group__header">
        <span className="today-group__index">{index}</span>
        <div>
          <h2 id={`today-${tone}`}>{title}</h2>
          <p>{description}</p>
        </div>
        <strong>{String(tasks.length).padStart(2, '0')}</strong>
      </header>
      <div className="today-group__tasks">
        {tasks.length ? (
          tasks.map((task) => (
            <TaskCard
              compact
              key={task.id ?? task.body}
              task={task}
              columns={snapshot.columns}
              busy={Boolean(task.id && busyKeys.has(`task:${task.id}`))}
              onSetCompleted={(id, completed) => void onSetCompleted(id, completed)}
              onMove={(id, column) => void onMove(id, column)}
            />
          ))
        ) : (
          <div className="today-group__empty">
            <span className="today-group__empty-line" />
            <p>{tone === 'overdue' ? '没有遗留项，节奏正常。' : tone === 'today' ? '今天没有到期项。' : '未来 7 天还没有排期。'}</p>
          </div>
        )}
      </div>
    </section>
  );
}

export function TodayView(props: TodayViewProps) {
  const { snapshot } = props;
  const { overdue, today, upcoming, next } = snapshot.today;
  const total = overdue.length + today.length + upcoming.length + next.length;
  return (
    <section className="today-view" aria-labelledby="today-title">
      <header className="view-heading view-heading--today">
        <div>
          <p className="eyebrow">DERIVED VIEW / {String(total).padStart(2, '0')}</p>
          <h1 id="today-title">今天，先做什么</h1>
        </div>
        <p className="view-heading__note">只从 Project Task 派生，不创建另一份任务。</p>
      </header>
      <div className="today-groups">
        <TodayGroup
          {...props}
          index="01"
          title="逾期"
          description="截止日期早于今天"
          tasks={overdue}
          tone="overdue"
        />
        <TodayGroup
          {...props}
          index="02"
          title="今天"
          description="需要在今日收口"
          tasks={today}
          tone="today"
        />
        <TodayGroup
          {...props}
          index="03"
          title="未来 7 天"
          description="近期即将到期"
          tasks={upcoming}
          tone="upcoming"
        />
        <TodayGroup
          {...props}
          index="04"
          title="后续可做"
          description="没有截止日期，按优先级排列"
          tasks={next}
          tone="next"
        />
      </div>
    </section>
  );
}
