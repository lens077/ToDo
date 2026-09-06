import type { DesktopColumn, DesktopSnapshot, DesktopTask } from './desktop';

/**
 * How the 目标 surface reads a project's columns.
 *
 * Columns are the user's own (`columns[]` in TODO.md frontmatter), so the
 * layout cannot assume "Todo / Doing / Done". It assumes only that one active
 * column is the work in flight; everything else that is not `type: done` is
 * queued work, and every `done` column is finished work.
 */
export interface GoalColumns {
  /** The column shown as work in flight. `null` when the file has no active column. */
  focus: DesktopColumn | null;
  focusColumns: DesktopColumn[];
  /** Active columns other than `focus`, in file order. */
  backlog: DesktopColumn[];
  /** `type: done` columns, in file order. */
  done: DesktopColumn[];
}

const FOCUS_HINT = /doing|progress|wip|active|current|now|正在|进行|推进|执行|处理中/i;

export function splitColumns(columns: DesktopColumn[]): GoalColumns {
  const paths = [...new Set(columns.map((column) => column.source_path))];
  if (paths.length > 1) {
    const groups = paths.map((source) => splitColumns(columns.filter((column) => column.source_path === source)));
    const focusColumns = groups.flatMap((group) => group.focusColumns);
    return { focus: focusColumns[0] ?? null, focusColumns, backlog: groups.flatMap((group) => group.backlog), done: groups.flatMap((group) => group.done) };
  }
  const active = columns.filter((column) => !column.done);
  const done = columns.filter((column) => column.done);
  // A column named like "Doing" wins; otherwise the second active column is
  // the conventional in-flight lane (Todo → Doing), and a single active column
  // is both queue and focus.
  const focus = active.find((column) => FOCUS_HINT.test(`${column.local_id ?? column.id} ${column.name}`)) ?? active[1] ?? active[0] ?? null;
  return {
    focus,
    focusColumns: focus ? [focus] : [],
    backlog: active.filter((column) => column !== focus),
    done,
  };
}

export const FOCUS_LIMIT_MIN = 1;
export const FOCUS_LIMIT_MAX = 5;
export const FOCUS_LIMIT_DEFAULT = 1;

export function clampFocusLimit(value: number): number {
  if (!Number.isFinite(value)) return FOCUS_LIMIT_DEFAULT;
  return Math.min(FOCUS_LIMIT_MAX, Math.max(FOCUS_LIMIT_MIN, Math.round(value)));
}

export interface GoalLayoutState {
  focusLimit: number;
  todoCollapsed: boolean;
  doneCollapsed: boolean;
}

export const GOAL_LAYOUT_STORAGE_KEY = 'todo.goal-layout.v1';

export const defaultGoalLayout: GoalLayoutState = {
  focusLimit: FOCUS_LIMIT_DEFAULT,
  todoCollapsed: false,
  doneCollapsed: false,
};

export function readGoalLayout(storage: Pick<Storage, 'getItem'> | null): GoalLayoutState {
  if (!storage) return defaultGoalLayout;
  try {
    const raw = storage.getItem(GOAL_LAYOUT_STORAGE_KEY);
    if (!raw) return defaultGoalLayout;
    const parsed = JSON.parse(raw) as Partial<GoalLayoutState>;
    return {
      focusLimit: clampFocusLimit(Number(parsed.focusLimit ?? FOCUS_LIMIT_DEFAULT)),
      todoCollapsed: Boolean(parsed.todoCollapsed),
      doneCollapsed: Boolean(parsed.doneCollapsed),
    };
  } catch {
    return defaultGoalLayout;
  }
}

export function writeGoalLayout(storage: Pick<Storage, 'setItem'> | null, state: GoalLayoutState): void {
  try {
    storage?.setItem(GOAL_LAYOUT_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Storage may be unavailable (private mode, quota); the layout still works
    // for this session.
  }
}

export type DueFilter = 'all' | 'overdue' | 'today' | 'upcoming';

/** Task ids in each derived due band, so the queue can filter without re-deriving dates. */
export function dueBands(snapshot: DesktopSnapshot): Record<Exclude<DueFilter, 'all'>, Set<string>> {
  const ids = (tasks: DesktopTask[]) => new Set(tasks.flatMap((task) => (task.id ? [task.id] : [])));
  return {
    overdue: ids(snapshot.today.overdue),
    today: ids(snapshot.today.today),
    upcoming: ids(snapshot.today.upcoming),
  };
}
