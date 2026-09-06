import * as React from 'react';
import { shortProjectName, type TaskSource } from '../lib/desktop';
import { Icon } from './Icon';

interface SourceEnrollmentDialogProps {
  projectPath: string;
  sources: TaskSource[];
  /** Currently enrolled paths; `null` on the first decision (everything shown). */
  enrolled: Set<string> | null;
  /** Paths discovered since the last decision. */
  pending: Set<string>;
  firstRun: boolean;
  onConfirm: (enrolled: string[]) => void;
  onCancel: () => void;
}

export function sourceStatusLabel(source: TaskSource): string {
  if (source.status === 'error') return '读取错误';
  if (source.status === 'unmanaged') return '未接管';
  return source.writable ? '已管理' : '只读';
}

function splitPath(path: string): [string, string] {
  const index = path.lastIndexOf('/');
  return index === -1 ? ['', path] : [path.slice(0, index + 1), path.slice(index + 1)];
}

/**
 * Chooses which discovered Task Sources join the board.
 *
 * Opening a directory that holds several projects surfaces every TODO.md in
 * it. This sheet makes that a decision rather than a default: the person ticks
 * the files that are their work, and the rest stay listed here for later.
 * Nothing is written to any file; enrollment is a per-machine preference.
 */
export function SourceEnrollmentDialog({ projectPath, sources, enrolled, pending, firstRun, onConfirm, onCancel }: SourceEnrollmentDialogProps) {
  const [checked, setChecked] = React.useState<Set<string>>(() => new Set(
    enrolled === null ? sources.map((source) => source.path) : sources.filter((source) => enrolled.has(source.path)).map((source) => source.path),
  ));
  const firstInput = React.useRef<HTMLInputElement>(null);
  React.useEffect(() => { firstInput.current?.focus(); }, []);
  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCancel();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onCancel]);

  const toggle = (path: string, on: boolean) => setChecked((current) => {
    const next = new Set(current);
    if (on) next.add(path); else next.delete(path);
    return next;
  });
  const managedPaths = sources.filter((source) => source.status === 'managed').map((source) => source.path);
  const count = checked.size;
  const taskTotal = sources.filter((source) => checked.has(source.path)).reduce((sum, source) => sum + source.task_count, 0);

  return (
    <div
      className="provider-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <section className="enroll-panel" role="dialog" aria-modal="true" aria-labelledby="enroll-title" aria-describedby="enroll-lede">
        <header className="enroll-panel__header">
          <div>
            <h2 id="enroll-title">{firstRun ? '录入任务来源' : '调整录入的来源'}</h2>
            <p id="enroll-lede">
              在 <strong>{shortProjectName(projectPath)}</strong> 发现 {sources.length} 个任务文件。勾选要汇入看板的来源；未勾选的仍在发现列表里，随时可以回来录入。这个选择只保存在本机。
            </p>
          </div>
          {!firstRun && (
            <button type="button" className="icon-action" aria-label="关闭" onClick={onCancel}>
              <Icon name="close" />
            </button>
          )}
        </header>

        <div className="enroll-toolbar" role="group" aria-label="批量选择">
          <button type="button" className="enroll-toolbar__action" onClick={() => setChecked(new Set(sources.map((source) => source.path)))}>全选</button>
          <button type="button" className="enroll-toolbar__action" disabled={managedPaths.length === 0} onClick={() => setChecked(new Set(managedPaths))}>只选已管理</button>
          <button type="button" className="enroll-toolbar__action" disabled={count === 0} onClick={() => setChecked(new Set())}>清空</button>
          <span className="enroll-toolbar__count" aria-live="polite">已选 {count} / {sources.length} · {taskTotal} 项任务</span>
        </div>

        <div className="enroll-list" role="group" aria-label="发现的任务来源">
          {sources.map((source, index) => {
            const on = checked.has(source.path);
            const isNew = pending.has(source.path);
            const [directory, file] = splitPath(source.path);
            return (
              <label key={source.path} className={`enroll-row${on ? ' is-on' : ''}`}>
                <input
                  ref={index === 0 ? firstInput : undefined}
                  type="checkbox"
                  checked={on}
                  onChange={(event) => toggle(source.path, event.target.checked)}
                  aria-label={`录入 ${source.path}`}
                />
                <span className="enroll-row__check" aria-hidden="true">{on && <Icon name="check" />}</span>
                <span className="enroll-row__copy">
                  <code className="enroll-row__path">{directory && <span>{directory}</span>}{file}</code>
                  {source.error && <small className="enroll-row__error">{source.error}</small>}
                </span>
                {isNew && <span className="enroll-row__new">新发现</span>}
                <span className={`enroll-row__status enroll-row__status--${source.status === 'managed' && source.writable ? 'managed' : source.status}`}>{sourceStatusLabel(source)}</span>
                <span className="enroll-row__count">{source.task_count} 项</span>
              </label>
            );
          })}
        </div>

        <footer className="enroll-panel__actions">
          {firstRun ? (
            <button type="button" className="text-action" onClick={() => onConfirm(sources.map((source) => source.path))}>全部录入</button>
          ) : (
            <button type="button" className="text-action" onClick={onCancel}>取消</button>
          )}
          <button type="button" className="solid-action" disabled={count === 0} title={count === 0 ? '至少录入一个来源' : undefined} onClick={() => onConfirm(Array.from(checked))}>
            {firstRun ? `录入所选（${count}）` : `保存（${count}）`}
          </button>
        </footer>
      </section>
    </div>
  );
}
