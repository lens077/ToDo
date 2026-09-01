import type { WorkspaceStatus } from '../hooks/useDesktop';
import { shortProjectName } from '../lib/desktop';
import { Icon, type IconName } from './Icon';

export type MainView = 'today' | 'board' | 'capture' | 'review';

interface SidebarProps {
  activeView: MainView;
  status: WorkspaceStatus;
  todayCount: number;
  captureCount: number;
  readOnly: boolean;
  onChangeView: (view: MainView) => void;
  onChooseProject: () => void;
}

// Today leads: the default landing view is "what should I do now", not the board.
const navItems: Array<{ id: MainView; label: string; icon: IconName }> = [
  { id: 'today', label: '今日', icon: 'calendar' },
  { id: 'board', label: '看板', icon: 'board' },
  { id: 'capture', label: '随记', icon: 'capture' },
  { id: 'review', label: '扫描与历史', icon: 'folder' },
];

export function Sidebar({
  activeView,
  status,
  todayCount,
  captureCount,
  readOnly,
  onChangeView,
  onChooseProject,
}: SidebarProps) {
  const projectName = status.projectPath ? shortProjectName(status.projectPath) : '未选择项目';
  const countFor = (view: MainView) => (view === 'today' ? todayCount : view === 'capture' ? captureCount : null);

  return (
    <aside className="sidebar">
      <div className="sidebar__brand" aria-label="Field Todo">
        <span className="sidebar__brand-mark">FT</span>
        <div>
          <strong>FIELD / TODO</strong>
          <span>LOCAL WORK MEMORY</span>
        </div>
      </div>

      <button
        className="project-switcher"
        type="button"
        onClick={onChooseProject}
        disabled={readOnly}
        title={readOnly ? '只读 Web 模式使用服务端当前项目' : '切换当前项目'}
      >
        <span className="project-switcher__icon"><Icon name="folder" /></span>
        <span className="project-switcher__copy">
          <small>当前项目</small>
          <strong>{projectName}</strong>
        </span>
        <Icon name="chevron" className="project-switcher__chevron" />
      </button>

      <nav className="primary-nav" aria-label="主要视图">
        <span className="primary-nav__label">工作视图</span>
        {navItems.map((item, index) => {
          const count = countFor(item.id);
          return (
            <button
              type="button"
              key={item.id}
              className={activeView === item.id ? 'is-active' : ''}
              aria-current={activeView === item.id ? 'page' : undefined}
              aria-label={item.label}
              title={item.label}
              onClick={() => onChangeView(item.id)}
            >
              <span className="primary-nav__index">0{index + 1}</span>
              <Icon name={item.icon} />
              <span className="primary-nav__text">{item.label}</span>
              {count !== null && count > 0 && <span className="primary-nav__count">{count}</span>}
            </button>
          );
        })}
      </nav>

      <div className="sidebar-status">
        <div className="sidebar-status__item">
          <span className={`status-light status-light--${status.syncState}`} />
          <div>
            <small>文件同步</small>
            <strong>{status.syncState === 'watching' ? '正在监听' : status.syncState === 'error' ? '需要处理' : '等待项目'}</strong>
          </div>
        </div>
        <div className="sidebar-status__item">
          <Icon name="command" />
          <div>
            <small>全局速记</small>
            <strong>{status.shortcutState === 'unavailable' ? '快捷键不可用' : '⌘ ⇧ Space'}</strong>
          </div>
        </div>
      </div>
    </aside>
  );
}
