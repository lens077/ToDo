import * as React from 'react';
import type { DesktopSnapshot } from '../lib/desktop';
import { Icon } from './Icon';
import { TaskCard } from './TaskCard';

interface BoardViewProps {
  snapshot: DesktopSnapshot;
  busyKeys: Set<string>;
  onSetCompleted: (id: string, completed: boolean) => Promise<boolean>;
  onMove: (id: string, column: string) => Promise<boolean>;
}

export function BoardView({ snapshot, busyKeys, onSetCompleted, onMove }: BoardViewProps) {
  const [draggedId, setDraggedId] = React.useState<string | null>(null);
  const [dropColumn, setDropColumn] = React.useState<string | null>(null);
  const [announcement, setAnnouncement] = React.useState('');

  if (snapshot.tasks.length === 0) {
    return (
      <section className="view-empty view-empty--tasks" aria-labelledby="empty-board-title">
        <span className="view-empty__index">00</span>
        <p className="eyebrow">TODO.md 已连接</p>
        <h2 id="empty-board-title">还没有 Project Task</h2>
        <p>从顶部快速新增第一项。它会写回当前项目，而不是留在界面缓存里。</p>
      </section>
    );
  }

  const moveTask = async (taskId: string, columnId: string) => {
    const task = snapshot.tasks.find((item) => item.id === taskId);
    if (!task || task.column === columnId) return;
    const destination = snapshot.columns.find((column) => column.id === columnId);
    setAnnouncement(`正在把“${task.body}”移动到${destination?.name ?? columnId}`);
    const succeeded = await onMove(taskId, columnId);
    setAnnouncement(
      succeeded
        ? `“${task.body}”已移动到${destination?.name ?? columnId}`
        : `“${task.body}”没有移动，仍保留原位置`,
    );
  };

  return (
    <section className="board-view" aria-labelledby="board-title">
      <header className="view-heading">
        <div>
          <p className="eyebrow">PROJECT TASKS / {snapshot.tasks.length}</p>
          <h1 id="board-title">工作台</h1>
        </div>
        <p className="view-heading__note">拖动只发送意图；文件写入成功后才会落位。</p>
      </header>

      <div className="board-scroll" tabIndex={0} aria-label="看板列，可横向滚动">
        <div className="board-columns">
          {snapshot.columns.map((column, columnIndex) => {
            const tasks = snapshot.tasks.filter((task) => task.column === column.id);
            const isDropTarget = dropColumn === column.id && draggedId !== null;
            return (
              <section
                className={`board-column${column.done ? ' board-column--done' : ''}${isDropTarget ? ' is-drop-target' : ''}`}
                key={column.id}
                onDragOver={(event) => {
                  if (!draggedId) return;
                  event.preventDefault();
                  event.dataTransfer.dropEffect = 'move';
                  setDropColumn(column.id);
                }}
                onDragLeave={(event) => {
                  if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropColumn(null);
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  const taskId = draggedId ?? event.dataTransfer.getData('text/plain');
                  setDraggedId(null);
                  setDropColumn(null);
                  if (taskId) void moveTask(taskId, column.id);
                }}
              >
                <header className="board-column__header">
                  <span className="board-column__index">{String(columnIndex + 1).padStart(2, '0')}</span>
                  <div>
                    <h2>{column.name}</h2>
                    <span>{tasks.length} 项</span>
                  </div>
                  {column.done && <Icon name="check" className="board-column__done-icon" />}
                </header>

                <div className="board-column__tasks">
                  {tasks.length > 0 ? (
                    tasks.map((task, taskIndex) => (
                      <div
                        className="task-card-wrap"
                        style={{ '--task-index': taskIndex } as React.CSSProperties}
                        key={task.id ?? `${column.id}-${taskIndex}`}
                      >
                        <TaskCard
                          task={task}
                          columns={snapshot.columns}
                          busy={Boolean(task.id && busyKeys.has(`task:${task.id}`))}
                          onDragStart={(id) => setDraggedId(id)}
                          onDragEnd={() => {
                            setDraggedId(null);
                            setDropColumn(null);
                          }}
                          onSetCompleted={(id, completed) => void onSetCompleted(id, completed)}
                          onMove={(id, destination) => void moveTask(id, destination)}
                        />
                      </div>
                    ))
                  ) : (
                    <div className="column-empty">
                      <span />
                      <p>{column.done ? '完成的工作会归档在这里' : '把 Project Task 拖到这里'}</p>
                    </div>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      </div>
      <p className="sr-only" aria-live="polite">
        {announcement}
      </p>
    </section>
  );
}
