/**
 * Which sources are read with the loose list reader (Beta).
 *
 * The reader treats plain `- text` bullets as tasks. Whether that is right
 * depends on how a particular file was written, so it is a per-source choice
 * the person makes and can undo at any time. It is a per-machine preference:
 * the file is never changed by turning it on, and the source stays read-only
 * until adoption rewrites the bullets into real checkboxes.
 */
export const LOOSE_SOURCES_STORAGE_PREFIX = 'todo.loose-sources.v1:';

function key(projectPath: string): string {
  return `${LOOSE_SOURCES_STORAGE_PREFIX}${projectPath}`;
}

export function readLooseSources(projectPath: string): string[] {
  try {
    const raw: unknown = JSON.parse(window.localStorage.getItem(key(projectPath)) ?? '[]');
    return Array.isArray(raw) ? Array.from(new Set(raw.filter((item): item is string => typeof item === 'string'))) : [];
  } catch {
    return [];
  }
}

export function writeLooseSources(projectPath: string, paths: string[]): void {
  try {
    window.localStorage.setItem(key(projectPath), JSON.stringify(Array.from(new Set(paths))));
  } catch {
    // Storage unavailable: the choice still holds for this session.
  }
}
