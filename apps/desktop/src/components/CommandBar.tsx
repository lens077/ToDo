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
  onOpenProviderSettings: () => void;
  onAddTask: (body: string, column: string | null) => Promise<boolean>;
  newTaskSource?: string | null;
  onNewTaskSource?: (source: string | null) => void;
}

export function CommandBar({
  snapshot,
  projectBusy,
  addBusy,
  readOnly,
  onChooseProject,
  onRefresh,
  onOpenProviderSettings,
  onAddTask,
  newTaskSource = null,
  onNewTaskSource,
}: CommandBarProps) {
  const [body, setBody] = React.useState('');
  const [column, setColumn] = React.useState(snapshot?.columns.find((item) => !item.done)?.id ?? '');
  const inputRef = React.useRef<HTMLInputElement>(null);

  const sourceReady = !snapshot?.sources || snapshot.sources.some((source) => source.path === newTaskSource && source.status === 'managed' && source.writable);
  const columns = React.useMemo(() => (snapshot?.columns ?? []).filter((item) => !snapshot?.sources || item.source_path === newTaskSource), [snapshot, newTaskSource]);
  React.useEffect(() => {
    if (!columns.some((item) => item.id === column)) setColumn(columns.find((item) => !item.done)?.id ?? columns[0]?.id ?? '');
  }, [column, columns]);

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
    if (readOnly || !snapshot || !sourceReady || !body.trim() || addBusy) return;
    const value = body;
    if (await onAddTask(value, column || null)) setBody('');
  };

  return (
    <header className="command-bar">
      <button
        className="command-project"
        type="button"
        disabled={projectBusy}
        onClick={onChooseProject}
        title={readOnly ? '查看项目连接方式（当前服务只读）' : '打开或切换项目'}
      >
        <Icon name="folder" />
        <span>{snapshot ? shortProjectName(snapshot.project_path) : '选择项目'}</span>
        <Icon name="chevron" />
      </button>

      <form
        className={`quick-add${snapshot?.sources ? ' has-sources' : ''}`}
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        {snapshot?.sources && <label className="quick-add__source">新增到来源
          <select aria-label="新增到来源" value={newTaskSource ?? ''} disabled={readOnly || addBusy} onChange={(event) => onNewTaskSource?.(event.target.value || null)}>
            <option value="">请选择来源文件</option>
            {snapshot.sources.filter((source) => source.status === 'managed' && source.writable).map((source) => <option key={source.path} value={source.path}>{source.path}</option>)}
          </select>
        </label>}
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
            {columns.length === 0 && <option value="">无列</option>}
            {columns.map((item) => (
              <option value={item.id} key={item.id}>{item.name}</option>
            ))}
          </select>
        )}
        <kbd>⌘K</kbd>
        <button type="submit" disabled={readOnly || !snapshot || !sourceReady || !body.trim() || addBusy}>
          {readOnly ? '只读' : addBusy ? '写入中' : '添加'}
        </button>
      </form>

      <div className="revision-chip" title={snapshot ? `Snapshot revision ${snapshot.revision}` : '尚无 Snapshot'}>
        <span>REV</span>
        <code>{snapshot ? revisionCode(snapshot.revision) : '—'}</code>
      </div>
      <div className="command-actions">
        <button className="text-action command-settings" type="button" onClick={onOpenProviderSettings} aria-label="模型设置" title="模型提供方设置">
          模型
        </button>
        <button className="icon-action command-refresh" type="button" disabled={!snapshot} onClick={onRefresh} aria-label="重新载入 Snapshot" title="重新载入">
          <Icon name="refresh" />
        </button>
      </div>
    </header>
  );
}
