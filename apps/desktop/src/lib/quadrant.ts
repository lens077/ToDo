import type { DesktopSnapshot, DesktopTask } from './desktop';

export interface QuadrantGroup {
  task: DesktopTask;
  children: QuadrantGroup[];
}

export function groupTasks(group: QuadrantGroup): DesktopTask[] {
  return [group.task, ...group.children.flatMap(groupTasks)];
}

/** Snapshot tasks retain file order; grouping must not change their priorities. */
export function quadrantGroups(snapshot: DesktopSnapshot): QuadrantGroup[] {
  const roots: QuadrantGroup[] = [];
  const ancestors: QuadrantGroup[] = [];
  for (const task of snapshot.tasks) {
    while (ancestors.length) {
      const parent = ancestors[ancestors.length - 1].task;
      if (parent.source_path === task.source_path && parent.column === task.column && parent.indent_level < task.indent_level) break;
      ancestors.pop();
    }
    const node: QuadrantGroup = { task, children: [] };
    const parent = ancestors[ancestors.length - 1];
    if (parent) parent.children.push(node);
    else roots.push(node);
    ancestors.push(node);
  }
  const doneColumns = new Set(snapshot.columns.filter((column) => column.done).map((column) => column.id));
  return roots.filter((group) => groupTasks(group).some((task) => !task.completed && !doneColumns.has(task.column ?? '')));
}
