/**
 * 工作空间: the multi-project registry view.
 *
 * The registry (not the filesystem) is the source of truth for project
 * identity: a project keeps its stable ID when its directory moves and is
 * re-linked here. Removing a project from a workspace never touches the
 * directory; deleting the registry entry is a separate, explicit action.
 */
import * as React from 'react';
import { workspace, type WorkspaceSummary } from '../lib/workspace';
import { Icon } from './Icon';
import { ProjectDetailPanel } from './ProjectDetailPanel';

interface WorkspaceViewProps {
  /** Opens a registered project by stable ID (swaps the live snapshot). */
  onOpenProject: (projectId: string) => Promise<void> | void;
  /** Path of the currently open project, if any. */
  openProjectPath?: string | null;
}

export function WorkspaceView({ onOpenProject, openProjectPath }: WorkspaceViewProps) {
  const [items, setItems] = React.useState<WorkspaceSummary[] | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busyKey, setBusyKey] = React.useState<string | null>(null);
  const [newName, setNewName] = React.useState('');
  const [creating, setCreating] = React.useState(false);
  const [expandedProjectId, setExpandedProjectId] = React.useState<string | null>(null);

  const refresh = React.useCallback(async () => {
    try {
      const list = await workspace.list();
      setItems(list);
      setError(null);
      if (list.length === 0) {
        await workspace.create('默认工作空间');
        setItems(await workspace.list());
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = React.useCallback(
    async (key: string, action: () => Promise<void>) => {
      setBusyKey(key);
      try {
        await action();
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setBusyKey(null);
      }
    },
    [],
  );

  const createWorkspace = React.useCallback(() => {
    const name = newName.trim();
    if (!name) return;
    void run('create', async () => {
      await workspace.create(name);
      setNewName('');
      setCreating(false);
      await refresh();
    });
  }, [newName, refresh, run]);

  const addProject = React.useCallback(
    (workspaceId: string) => {
      void run(`add:${workspaceId}`, async () => {
        const path = await workspace.pickDirectory('选择项目根目录');
        if (!path) return;
        const added = await workspace.addProject(workspaceId, path);
        await refresh();
        if (added.created) void onOpenProject(added.project.id);
      });
    },
    [onOpenProject, refresh, run],
  );

  const relinkProject = React.useCallback(
    (projectId: string) => {
      void run(`relink:${projectId}`, async () => {
        const path = await workspace.pickDirectory('重新关联项目目录');
        if (!path) return;
        await workspace.relinkProject(projectId, path);
        await refresh();
      });
    },
    [refresh, run],
  );

  if (items === null) {
    return (
      <section className="reading-view">
        <p className="ai-loading" role="status">{error ?? '正在载入工作空间…'}</p>
      </section>
    );
  }

  return (
    <section className="reading-view ws-view">
      <div className="ws-toolbar">
        <h2 className="ws-title">工作空间</h2>
        <div className="ws-toolbar__actions">
          <button className="button-primary" type="button" onClick={() => setCreating((open) => !open)}>
            <Icon name="plus" /> 新建工作空间
          </button>
        </div>
      </div>
      {creating && (
        <form
          className="ws-create"
          onSubmit={(event) => {
            event.preventDefault();
            createWorkspace();
          }}
        >
          <input
            autoFocus
            value={newName}
            placeholder="工作空间名称，例如 Personal、Client Work"
            onChange={(event) => setNewName(event.target.value)}
          />
          <button className="button-primary" type="submit" disabled={!newName.trim()}>创建</button>
        </form>
      )}
      {error && <p className="ai-notice" role="alert">{error}</p>}
      {items.map((item) => (
        <div className="ws-workspace" key={item.id}>
          <div className="ws-workspace__head">
            <h3>{item.name}</h3>
            <div className="ws-workspace__tools">
              <button
                type="button"
                className="ws-tool"
                title="添加项目目录"
                disabled={busyKey !== null}
                onClick={() => addProject(item.id)}
              >
                <Icon name="plus" /> 添加项目
              </button>
              <button
                type="button"
                className="ws-tool ws-tool--danger"
                title="删除工作空间（项目保留）"
                disabled={busyKey !== null}
                onClick={() => {
                  if (!window.confirm(`删除工作空间「${item.name}」？项目记录与目录都会保留。`)) return;
                  void run(`del:${item.id}`, async () => {
                    await workspace.remove(item.id);
                    await refresh();
                  });
                }}
              >
                <Icon name="trash" />
              </button>
            </div>
          </div>
          {item.projects.length === 0 ? (
            <p className="ai-empty">还没有项目。添加一个真实目录，或从其他工作空间共享项目。</p>
          ) : (
            <div className="reading-list ws-projects">
              {item.projects.map((project) => {
                const missing = project.path_status !== 'reachable';
                const active = openProjectPath === project.root_path;
                return (
                  <React.Fragment key={project.id}>
                  <article
                    className={`reading-card ws-project${missing ? ' ws-project--missing' : ''}`}
                  >
                    <div className="ws-project__main">
                      <div className="ws-project__title-row">
                        <strong className="ws-project__name">{project.name}</strong>
                        <span className={`ws-badge ws-badge--${project.path_status}`}>
                          {missing ? '目录缺失' : '正常'}
                        </span>
                        {active && <span className="ws-badge ws-badge--active">当前打开</span>}
                      </div>
                      <p className="ws-project__path" title={project.root_path}>{project.root_path}</p>
                      <p className="ws-project__meta">
                        ID <code>{project.id}</code>
                        {project.workspace_ids.length > 1 && (
                          <> · 属于 {project.workspace_ids.length} 个工作空间</>
                        )}
                      </p>
                    </div>
                    <div className="ws-project__actions">
                      <button
                        type="button"
                        className="ws-tool"
                        aria-expanded={expandedProjectId === project.id}
                        onClick={() =>
                          setExpandedProjectId((current) => (current === project.id ? null : project.id))
                        }
                      >
                        <Icon name="panel-right" /> 档案
                      </button>
                      {missing ? (
                        <button type="button" className="button-primary" onClick={() => relinkProject(project.id)}>
                          <Icon name="refresh" /> 重新关联
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="button-primary"
                          disabled={busyKey !== null}
                          onClick={() => void onOpenProject(project.id)}
                        >
                          打开
                        </button>
                      )}
                      <div className="ws-project__tools">
                        <button
                          type="button"
                          className="ws-tool"
                          title="从该工作空间移除（项目记录保留）"
                          onClick={() => {
                            void run(`rmws:${project.id}`, async () => {
                              await workspace.removeProject(item.id, project.id);
                              await refresh();
                            });
                          }}
                        >
                          <Icon name="close" />
                        </button>
                        {missing && (
                          <button type="button" className="ws-tool" onClick={() => relinkProject(project.id)}>
                            <Icon name="refresh" />
                          </button>
                        )}
                        <button
                          type="button"
                          className="ws-tool ws-tool--danger"
                          title="从注册表删除项目（目录不受影响）"
                          onClick={() => {
                            if (!window.confirm(`从注册表删除「${project.name}」？磁盘目录不会被删除。`)) return;
                            void run(`delproj:${project.id}`, async () => {
                              await workspace.deleteProject(project.id);
                              await refresh();
                            });
                          }}
                        >
                          <Icon name="trash" />
                        </button>
                      </div>
                    </div>
                  </article>
                  {expandedProjectId === project.id && (
                    <ProjectDetailPanel projectId={project.id} projectName={project.name} />
                  )}
                  </React.Fragment>
                );
              })}
            </div>
          )}
        </div>
      ))}
    </section>
  );
}
