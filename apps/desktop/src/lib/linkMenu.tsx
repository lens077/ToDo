// 正文里的链接点击：统一弹出「在浏览器打开 / 复制链接」菜单。阅读页、工作空间
// 文档预览、任务正文共用这一套，避免每处自己写一遍定位与收起的逻辑。
import React from 'react';
import { Icon } from '../components/Icon';
import { desktop } from './desktop';

interface LinkTarget {
  href: string;
  x: number;
  y: number;
}

const MENU_WIDTH = 196;
const MENU_HEIGHT = 96;

export interface LinkMenuApi {
  /** 菜单节点，渲染到组件里即可；没有打开时不渲染。 */
  node: React.ReactNode;
  /** 点击事件里命中 http(s) 链接时返回 true（已 preventDefault 并弹菜单）。 */
  openFromEvent: (event: React.MouseEvent<HTMLElement>) => boolean;
  close: () => void;
}

export function useLinkMenu(): LinkMenuApi {
  const [target, setTarget] = React.useState<LinkTarget | null>(null);
  const [copied, setCopied] = React.useState(false);
  const menuRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    if (!target) return undefined;
    const onPointerDown = (event: PointerEvent) => {
      if (menuRef.current?.contains(event.target as Node)) return;
      setTarget(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setTarget(null);
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [target]);

  const openFromEvent = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    const anchor = (event.target as HTMLElement).closest<HTMLAnchorElement>('a[href]');
    if (!anchor) return false;
    // 用原始属性而不是 anchor.href：相对链接在 Tauri 里会被解析成应用自身
    // 的 origin，不该当外链处理。
    const href = anchor.getAttribute('href') ?? '';
    if (!/^https?:\/\//i.test(href)) return false;
    event.preventDefault();
    event.stopPropagation();
    const rect = anchor.getBoundingClientRect();
    const x = Math.min(Math.max(8, rect.left), window.innerWidth - MENU_WIDTH - 8);
    const above = window.innerHeight - rect.bottom < MENU_HEIGHT + 12;
    const y = above ? Math.max(8, rect.top - MENU_HEIGHT - 6) : rect.bottom + 6;
    setCopied(false);
    setTarget({ href, x, y });
    return true;
  }, []);

  const open = () => {
    if (!target) return;
    void desktop.openExternal(target.href);
    setTarget(null);
  };

  const copy = async () => {
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.href);
      setCopied(true);
      window.setTimeout(() => setTarget(null), 700);
    } catch {
      setCopied(false);
    }
  };

  const node = target ? (
    <div className="reader-link-menu" role="menu" aria-label="链接操作" ref={menuRef} style={{ left: target.x, top: target.y }}>
      <button type="button" role="menuitem" onClick={open}>
        <Icon name="external" /> 在浏览器打开
      </button>
      <button type="button" role="menuitem" onClick={() => void copy()}>
        <Icon name="link" /> {copied ? '已复制' : '复制链接'}
      </button>
    </div>
  ) : null;

  return { node, openFromEvent, close: () => setTarget(null) };
}
