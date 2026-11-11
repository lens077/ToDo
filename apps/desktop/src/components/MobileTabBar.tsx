import * as React from 'react';
import { useAppSettings } from '../lib/appSettings';
import { navItems, type MainView } from './Sidebar';
import { Icon } from './Icon';

/** Hold a tab this long to jump straight into the tab editor. */
const HOLD_TO_EDIT_MS = 400;

interface MobileTabBarProps {
  activeView: MainView;
  /** Badge numbers keyed by view; only views actually in the bar show one. */
  counts: Partial<Record<MainView, number>>;
  onChangeView: (view: MainView) => void;
  onOpenSettings: () => void;
  /** Long-press on any non-fixed tab: open settings at the tab editor. */
  onEditTabs: () => void;
}

/**
 * Narrow-screen tab bar. Two to six customizable views plus a fixed 「我的」
 * slot on the far right, which is the way into settings and every view that
 * did not fit in the bar.
 */
export function MobileTabBar({ activeView, counts, onChangeView, onOpenSettings, onEditTabs }: MobileTabBarProps) {
  const [settings] = useAppSettings();
  const tabs = settings.mobileTabBar.order;
  const holdRef = React.useRef<{ x: number; y: number; timer: number } | null>(null);
  const suppressClickRef = React.useRef(false);

  const cancelHold = React.useCallback(() => {
    if (!holdRef.current) return;
    window.clearTimeout(holdRef.current.timer);
    holdRef.current = null;
  }, []);

  React.useEffect(() => cancelHold, [cancelHold]);

  const startHold = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    const { clientX: x, clientY: y } = event;
    holdRef.current = {
      x,
      y,
      timer: window.setTimeout(() => {
        holdRef.current = null;
        // The click that follows a hold must not also switch views.
        suppressClickRef.current = true;
        onEditTabs();
      }, HOLD_TO_EDIT_MS),
    };
  };

  const moveHold = (event: React.PointerEvent<HTMLButtonElement>) => {
    const hold = holdRef.current;
    if (hold && (Math.abs(event.clientX - hold.x) > 8 || Math.abs(event.clientY - hold.y) > 8)) cancelHold();
  };

  const choose = (view: MainView) => {
    if (suppressClickRef.current) {
      suppressClickRef.current = false;
      return;
    }
    onChangeView(view);
  };

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
        const count = counts[id] ?? 0;
        return (
          <button
            type="button"
            key={id}
            className={active ? 'is-active' : ''}
            aria-current={active ? 'page' : undefined}
            aria-label={item.label}
            onPointerDown={startHold}
            onPointerMove={moveHold}
            onPointerUp={cancelHold}
            onPointerCancel={cancelHold}
            onPointerLeave={cancelHold}
            onClick={() => choose(id)}
          >
            <Icon name={item.icon} />
            <span>{item.label}</span>
            {count > 0 && <span className="mobile-tab-bar__count">{count}</span>}
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
