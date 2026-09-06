import type { DesktopMutation, DesktopSnapshot, DesktopTask } from './desktop';

// JSON tuple encoding is unambiguous even when paths contain punctuation.
export function sourceKey(source: string | undefined, id: string): string {
  return source === undefined ? id : JSON.stringify([source, id]);
}

export function taskWritable(task: DesktopTask): boolean {
  return task.source_writable !== false;
}

export function requireSource(snapshot: DesktopSnapshot, source: string | null): void {
  if (!snapshot.sources) return;
  const entry = snapshot.sources.find((item) => item.path === source);
  if (!entry || entry.status !== 'managed' || !entry.writable) {
    throw new Error('请明确选择一个可写的已管理来源文件。');
  }
}

export function sourceMutation(snapshot: DesktopSnapshot, intent: DesktopMutation, newSource: string | null): { source_path: string | null; intent: DesktopMutation } {
  if (!snapshot.sources) return { source_path: null, intent };
  const task = 'id' in intent ? snapshot.tasks.find((item) => item.id === intent.id) : undefined;
  if ('id' in intent && !task) throw new Error('任务已不在当前来源，请刷新后重试。');
  const source = task?.source_path ?? newSource;
  requireSource(snapshot, source);
  if (task && (!taskWritable(task) || !task.local_id)) throw new Error('这个来源只读，不能修改任务。');
  let local: DesktopMutation = 'id' in intent ? { ...intent, id: task!.local_id! } : { ...intent };
  if ('column' in local && local.column !== null) {
    const columnKey = local.column;
    const column = snapshot.columns.find((item) => item.id === columnKey);
    if (!column || column.source_path !== source) throw new Error('不能跨来源文件移动或新增任务，请选择同一文件的列。');
    local = { ...local, column: column.local_id! };
  }
  if (local.type === 'move_task' && (local.target.type === 'before' || local.target.type === 'after')) {
    const targetKey = local.target.task_id;
    const target = snapshot.tasks.find((item) => item.id === targetKey);
    if (!target?.local_id || target.source_path !== source) throw new Error('不能跨来源文件排序。');
    local = { ...local, target: { ...local.target, task_id: target.local_id } };
  }
  return { source_path: source, intent: local };
}

export function sourceQuery(path: string, source?: string | null): string {
  return source ? `${path}${path.includes('?') ? '&' : '?'}source=${encodeURIComponent(source)}` : path;
}
