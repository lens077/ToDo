import * as React from 'react';
import { shortProjectName, type DesktopSnapshot } from '../lib/desktop';
import { revisionCode } from '../lib/format';
import { Icon } from './Icon';

interface CommandBarProps {
  snapshot: DesktopSnapshot | null;
  projectBusy: boolean;
  addBusy: boolean;
  readOnly: boolean;
  onChooseProject: () => void;
  onRefresh: () => void;
  onAddTask: (body: string, column: string | null) => Promise<boolean>;
}

export function CommandBar({
  snapshot,
  projectBusy,
  addBusy,
  readOnly,
  onChooseProject,
  onRefresh,
  onAddTask,
}: CommandBarProps) {
  const [body, setBody] = React.useState('');
  const [column, setColumn] = React.useState(snapshot?.columns.find((item) => !item.done)?.id ?? '');
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    const columns = snapshot?.columns ?? [];
    if (!columns.some((item) => item.id === column)) setColumn(columns.find((item) => !item.done)?.id ?? columns[0]?.id ?? '');
  }, [column, snapshot]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!readOnly && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [readOnly]);

  const submit = async () => {
    if (readOnly || !snapshot || !body.trim() || addBusy) return;
    const value = body;
    if (await onAddTask(value, column || null)) setBody('');
  };

  return (
    <header className="command-bar">
      <button
        className="command-project"
        type="button"
        disabled={projectBusy || readOnly}
        onClick={onChooseProject}
        title={readOnly ? '只读 Web 模式使用服务端当前项目' : '切换项目'}
      >
        <Icon name="folder" />
        <span>{snapshot ? shortProjectName(snapshot.project_path) : '选择项目'}</span>
        <Icon name="chevron" />
      </button>

      <form
        className="quick-add"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <Icon name="plus" />
        <label className="sr-only" htmlFor="quick-add-body">快速新增 Project Task</label>
        <input
          ref={inputRef}
          id="quick-add-body"
          value={body}
          onChange={(event) => setBody(event.target.value)}
          placeholder={readOnly ? '只读模式下不能新增任务' : snapshot ? '快速新增 Project Task…' : '先选择一个项目'}
          disabled={readOnly || !snapshot || addBusy}
          autoComplete="off"
        />
        {snapshot && (
          <select aria-label="新增到列" value={column} onChange={(event) => setColumn(event.target.value)} disabled={readOnly || addBusy}>
            {snapshot.columns.map((item) => (
              <option value={item.id} key={item.id}>{item.name}</option>
            ))}
          </select>
        )}
        <kbd>⌘K</kbd>
        <button type="submit" disabled={readOnly || !snapshot || !body.trim() || addBusy}>
          {readOnly ? '只读' : addBusy ? '写入中' : '添加'}
        </button>
      </form>

      <div className="revision-chip" title={snapshot ? `Snapshot revision ${snapshot.revision}` : '尚无 Snapshot'}>
        <span>REV</span>
        <code>{snapshot ? revisionCode(snapshot.revision) : '—'}</code>
      </div>
      <button className="icon-action command-refresh" type="button" disabled={!snapshot} onClick={onRefresh} aria-label="重新载入 Snapshot" title="重新载入">
        <Icon name="refresh" />
      </button>
    </header>
  );
}
