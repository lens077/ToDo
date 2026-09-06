import * as React from 'react';
import type { DesktopSnapshot, TaskSource } from './desktop';

/**
 * Which discovered Task Sources take part in the aggregate view.
 *
 * A project root that contains several projects also contains several
 * TODO.md files. Discovery still reports every one of them (the file list is
 * product truth), but the person opening the root decides which of them are
 * *their* work. That decision is a per-machine preference keyed by project
 * path: it never enters any file, and clearing the browser profile simply
 * shows the enrollment sheet again.
 *
 * `seen` records every path the person has been shown, so a TODO.md that
 * appears later is reported as new instead of silently joining the board.
 */
export interface SourceEnrollment {
  enrolled: string[];
  seen: string[];
}

export const SOURCE_ENROLLMENT_STORAGE_PREFIX = 'todo.source-enrollment.v1:';

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

function key(projectPath: string): string {
  return `${SOURCE_ENROLLMENT_STORAGE_PREFIX}${projectPath}`;
}

function stringList(value: unknown): string[] | null {
  return Array.isArray(value) && value.every((item) => typeof item === 'string') ? Array.from(new Set(value as string[])) : null;
}

export function readSourceEnrollment(projectPath: string): SourceEnrollment | null {
  const store = storage();
  if (!store) return null;
  try {
    const raw: unknown = JSON.parse(store.getItem(key(projectPath)) ?? 'null');
    if (!raw || typeof raw !== 'object') return null;
    const enrolled = stringList((raw as Partial<SourceEnrollment>).enrolled);
    const seen = stringList((raw as Partial<SourceEnrollment>).seen);
    if (!enrolled) return null;
    return { enrolled, seen: seen ?? enrolled };
  } catch {
    return null;
  }
}

export function writeSourceEnrollment(projectPath: string, value: SourceEnrollment): void {
  try {
    storage()?.setItem(key(projectPath), JSON.stringify(value));
  } catch {
    // Storage unavailable: the choice still holds for this session.
  }
}

export interface EnrollmentState {
  /** `null` until the project has more than one source or a decision exists. */
  enrolled: Set<string> | null;
  /** Discovered since the last decision; not on the board until enrolled. */
  pending: TaskSource[];
  /** True when the project has several sources and no decision was recorded. */
  needsDecision: boolean;
}

export function resolveEnrollment(snapshot: DesktopSnapshot | null, stored: SourceEnrollment | null): EnrollmentState {
  const sources = snapshot?.sources;
  if (!sources || sources.length === 0) return { enrolled: null, pending: [], needsDecision: false };
  if (!stored) {
    return sources.length > 1
      ? { enrolled: null, pending: [], needsDecision: true }
      : { enrolled: null, pending: [], needsDecision: false };
  }
  const seen = new Set(stored.seen);
  const enrolled = new Set(stored.enrolled.filter((path) => sources.some((source) => source.path === path)));
  const pending = sources.filter((source) => !seen.has(source.path));
  return { enrolled, pending, needsDecision: false };
}

/**
 * The snapshot as the board should read it: only enrolled sources, their
 * columns and tasks. `sources` keeps the enrolled entries so every selector
 * downstream lists only work the person chose to see.
 */
export function filterSnapshotBySources(snapshot: DesktopSnapshot, enrolled: Set<string> | null): DesktopSnapshot {
  if (!snapshot.sources || enrolled === null) return snapshot;
  const keep = (path: string | undefined) => path !== undefined && enrolled.has(path);
  const tasks = snapshot.tasks.filter((task) => keep(task.source_path));
  return {
    ...snapshot,
    sources: snapshot.sources.filter((source) => enrolled.has(source.path)),
    columns: snapshot.columns.filter((column) => keep(column.source_path)),
    tasks,
    today: {
      overdue: snapshot.today.overdue.filter((task) => keep(task.source_path)),
      today: snapshot.today.today.filter((task) => keep(task.source_path)),
      upcoming: snapshot.today.upcoming.filter((task) => keep(task.source_path)),
      next: snapshot.today.next.filter((task) => keep(task.source_path)),
    },
  };
}

const listeners = new Set<() => void>();
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
let version = 0;

export function commitSourceEnrollment(projectPath: string, enrolled: string[], seen: string[]): void {
  writeSourceEnrollment(projectPath, { enrolled, seen: Array.from(new Set([...seen, ...enrolled])) });
  version += 1;
  listeners.forEach((listener) => listener());
}

/** Live enrollment for the open project; re-resolves on every commit and snapshot. */
export function useSourceEnrollment(snapshot: DesktopSnapshot | null): EnrollmentState {
  const tick = React.useSyncExternalStore(subscribe, () => version, () => 0);
  return React.useMemo(() => {
    const stored = snapshot ? readSourceEnrollment(snapshot.project_path) : null;
    return resolveEnrollment(snapshot, stored);
    // `tick` is the invalidation signal for storage writes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, tick]);
}
