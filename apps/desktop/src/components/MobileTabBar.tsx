import type * as React from 'react';
import { useAppSettings } from '../lib/appSettings';
import { navItems, type MainView } from './Sidebar';
import { Icon } from './Icon';

/**
 * Narrow-screen tab bar. Two to six customizable views plus a fixed 「我的」
 * slot on the far right, which is the way into settings and every view that
 * did not fit in the bar.
 *
 * The order comes from device-local preferences; M3 adds the drag editor.
 */
interface MobileTabBarProps {
  activeView: MainView;
  onChangeView: (view: MainView) => void;
  onOpenSettings: () => void;
}

export function MobileTabBar({ activeView, onChangeView, onOpenSettings }: MobileTabBarProps) {
  const [settings] = useAppSettings();
  const tabs = settings.mobileTabBar.order;

  return (
    <nav
      className="mobile-tab-bar"
      aria-label="底部导航"
      style={{ '--mobile-tab-count': tabs.length } as React.CSSProperties}
    >
      {tabs.map((id) => {
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
