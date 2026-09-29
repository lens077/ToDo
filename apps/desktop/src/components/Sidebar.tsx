import type { WorkspaceStatus } from '../hooks/useDesktop';
import { shortProjectName } from '../lib/desktop';
import { Icon, type IconName } from './Icon';

export type MainView = 'reading' | 'goal' | 'quadrant' | 'capture' | 'ai' | 'review';

interface SidebarProps {
  activeView: MainView;
  status: WorkspaceStatus;
  /** Saved articles not yet finished: the count shown beside 阅读. */
  readingCount: number;
  /** Tasks in flight right now: the Doing count shown beside 目标. */
  goalCount: number;
  captureCount: number;
  unrankedCount: number;
  /** Agent sessions waiting, failed, or finished unread in the current scope. */
  aiCount: number;
  readOnly: boolean;
  onChangeView: (view: MainView) => void;
  onChooseProject: () => void;
  onOpenSettings: () => void;
}

// 阅读 sits first: it is the personal shelf that feeds everything below it.
// 目标 then holds what is in flight, what is queued, and what is finished, so
// "what should I do now" never needs a second view.
const navItems: Array<{ id: MainView; label: string; icon: IconName }> = [
  { id: 'reading', label: '阅读', icon: 'book' },
  { id: 'goal', label: '目标', icon: 'target' },
  { id: 'quadrant', label: '象限', icon: 'grid' },
  { id: 'capture', label: '随记', icon: 'capture' },
  { id: 'ai', label: 'AI', icon: 'sparkle' },
  { id: 'review', label: '扫描与历史', icon: 'folder' },
];

export function Sidebar({
  activeView,
  status,
  readingCount,
  goalCount,
  captureCount,
  unrankedCount,
  aiCount,
  readOnly,
  onChangeView,
  onChooseProject,
  onOpenSettings,
}: SidebarProps) {
  const projectName = status.projectPath ? shortProjectName(status.projectPath) : '未选择项目';
  const countFor = (view: MainView) => (view === 'reading' ? readingCount : view === 'goal' ? goalCount : view === 'quadrant' ? unrankedCount : view === 'capture' ? captureCount : view === 'ai' ? aiCount : null);

  return (
    <aside className="sidebar">
      <div className="sidebar__brand" aria-label="Sumery Virgo">
        <picture><source srcSet="./favicon-dark.png" media="(prefers-color-scheme: dark)" /><img alt="" className="sidebar__brand-mark" height={22} src="./favicon-light.png" width={22} /></picture>
        <div>
          <span className="sidebar__brand-kicker">SUMERY · VIRGO</span>
          <strong>Virgo</strong>
        </div>
      </div>

      <button
        className="project-switcher"
        type="button"
        onClick={onChooseProject}
        title={readOnly ? '查看项目连接方式（当前服务只读）' : '打开或切换项目'}
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
        {navItems.map((item) => {
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
        <button type="button" className="sidebar-settings" onClick={onOpenSettings} aria-label="设置" title="设置">
          <Icon name="settings" />
          <span>设置</span>
        </button>
      </div>
    </aside>
  );
}
