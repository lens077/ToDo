/**
 * 阅读正文右键菜单（任务 sv2md9）：另存为 HTML / Markdown。
 * 复用任务右键菜单的样式骨架（task-context-menu）；导出逻辑在 useArticleExport。
 */
import * as React from 'react';
import { createPortal } from 'react-dom';
import type { ReadingExportFormat } from '../lib/reading';
import { Icon } from './Icon';

interface ReaderContextMenuProps {
  x: number;
  y: number;
  onExport: (format: ReadingExportFormat) => void;
  onClose: () => void;
}

export function ReaderContextMenu({ x, y, onExport, onClose }: ReaderContextMenuProps) {
  const menuRef = React.useRef<HTMLDivElement>(null);
  const [position, setPosition] = React.useState({ x, y });

  React.useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const rect = menu.getBoundingClientRect();
    const gutter = 8;
    setPosition({
      x: Math.max(gutter, Math.min(x, window.innerWidth - rect.width - gutter)),
      y: Math.max(gutter, Math.min(y, window.innerHeight - rect.height - gutter)),
    });
    menu.querySelector<HTMLButtonElement>('button[role="menuitem"]')?.focus();
  }, [x, y]);

  React.useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const close = () => onClose();
    window.addEventListener('pointerdown', closeOutside, true);
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('pointerdown', closeOutside, true);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
    };
  }, [onClose]);

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault();
      onClose();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]') ?? []);
      const index = Math.max(0, items.indexOf(event.target as HTMLButtonElement));
      items[(index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
    }
  };

  const run = (format: ReadingExportFormat) => {
    // 先关菜单再导出，避免目录对话框弹出时菜单还挂着焦点。
    onClose();
    onExport(format);
  };

  return createPortal(
    <div
      ref={menuRef}
      className="task-context-menu reader-context-menu"
      role="menu"
      aria-label="另存文章"
      style={{ left: position.x, top: position.y }}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <button type="button" role="menuitem" onClick={() => run('html')}>
        <Icon name="save" />
        <span>另存为 HTML</span>
      </button>
      <button type="button" role="menuitem" onClick={() => run('markdown')}>
        <Icon name="save" />
        <span>另存为 Markdown</span>
      </button>
    </div>,
    document.body,
  );
}
