import { navItems, type MainView } from './Sidebar';
import { Icon } from './Icon';

/**
 * Narrow-screen tab bar. Four customizable views plus a fixed 「我的」 slot on
 * the far right, which is the one way into settings and (later) every view
 * that does not fit in the bar.
 *
 * M1 hardcodes the default order; persistence and the editor land in M2/M3.
 */
export const DEFAULT_MOBILE_TABS: MainView[] = ['goal', 'reading', 'capture', 'overview'];

interface MobileTabBarProps {
  activeView: MainView;
  onChangeView: (view: MainView) => void;
  onOpenSettings: () => void;
}

export function MobileTabBar({ activeView, onChangeView, onOpenSettings }: MobileTabBarProps) {
  return (
    <nav className="mobile-tab-bar" aria-label="底部导航">
      {DEFAULT_MOBILE_TABS.map((id) => {
        const item = navItems.find((candidate) => candidate.id === id);
        if (!item) return null;
        const active = activeView === id;
        return (
          <button
            type="button"
            key={id}
            className={active ? 'is-active' : ''}
            aria-current={active ? 'page' : undefined}
            aria-label={item.label}
            onClick={() => onChangeView(id)}
          >
            <Icon name={item.icon} />
            <span>{item.label}</span>
          </button>
        );
      })}
      <button
        type="button"
        className="mobile-tab-bar__me"
        aria-label="我的"
        title="我的"
        onClick={onOpenSettings}
      >
        <Icon name="settings" />
        <span>我的</span>
      </button>
    </nav>
  );
}
