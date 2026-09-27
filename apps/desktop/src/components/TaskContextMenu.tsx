import * as React from 'react';
import { plainInlineMarkdown } from '../lib/inlineMarkdown';
import { createPortal, flushSync } from 'react-dom';
import type { DesktopColumn, DesktopTask, TaskPriority } from '../lib/desktop';
import { Icon } from './Icon';

export interface TaskMoveTargets {
  todo: DesktopColumn | null;
  doing: DesktopColumn | null;
  done: DesktopColumn | null;
}

interface TaskContextMenuProps {
  task: DesktopTask;
  columns: DesktopColumn[];
  targets: TaskMoveTargets;
  x: number;
  y: number;
  onMove: (columnId: string) => void;
  onSetPriority?: (priority: TaskPriority) => void;
  onConfigureColumns?: () => void;
  onStartUnclassified?: () => void;
  allowCurrentPriority?: boolean;
  onClose: (restoreFocus?: boolean) => void;
}

interface QuickAction {
  label: string;
  column: DesktopColumn;
  icon: 'arrow' | 'check';
}

function enabledItems(menu: HTMLElement): HTMLButtonElement[] {
  return Array.from(
    menu.querySelectorAll<HTMLButtonElement>(':scope > button[role="menuitem"]:not(:disabled)'),
  );
}

function focusRelative(menu: HTMLElement, current: Element | null, offset: number) {
  const items = enabledItems(menu);
  if (items.length === 0) return;
  const index = Math.max(0, items.indexOf(current as HTMLButtonElement));
  items[(index + offset + items.length) % items.length]?.focus();
}

function quickActions(task: DesktopTask, targets: TaskMoveTargets): QuickAction[] {
  const finished = task.completed;
  const actions: QuickAction[] = [];
  const add = (label: string, column: DesktopColumn | null, icon: QuickAction['icon']) => {
    if (!column || actions.some((action) => action.column.id === column.id)) return;
    actions.push({ label, column, icon });
  };

  if (finished) {
    if (targets.done && targets.done.id !== task.column) {
      add(`归档到（${targets.done.name}）`, targets.done, 'check');
    }
    add(`继续处理（${targets.doing?.name ?? 'Doing'}）`, targets.doing, 'arrow');
    add(`恢复到 ${targets.todo?.name ?? 'Todo'}`, targets.todo, 'arrow');
  } else {
    if (targets.doing && targets.doing.id !== task.column) {
      add(`开始处理（${targets.doing.name}）`, targets.doing, 'arrow');
    }
    if (targets.todo && targets.todo.id !== task.column) {
      add(`移回 ${targets.todo.name}`, targets.todo, 'arrow');
    }
    add(`标为完成（${targets.done?.name ?? 'Done'}）`, targets.done, 'check');
  }
  return actions;
}

/** Pointer-positioned menu shared by right click, keyboard, and the card button. */
export function TaskContextMenu({
  task,
  columns,
  targets,
  x,
  y,
  onMove,
  onSetPriority,
  onConfigureColumns,
  onStartUnclassified,
  allowCurrentPriority = false,
  onClose,
}: TaskContextMenuProps) {
  const menuRef = React.useRef<HTMLDivElement>(null);
  const submenuTriggerRef = React.useRef<HTMLButtonElement>(null);
  const priorityTriggerRef = React.useRef<HTMLButtonElement>(null);
  const priorityRef = React.useRef<HTMLDivElement>(null);
  const submenuRef = React.useRef<HTMLDivElement>(null);
  const [submenu, setSubmenu] = React.useState<{
    kind: 'move' | 'priority';
    focusFirst: boolean;
  } | null>(null);
  const submenuOpen = submenu?.kind === 'move';
  const priorityOpen = submenu?.kind === 'priority';
  const [submenuPosition, setSubmenuPosition] = React.useState({ x: 0, y: 0 });
  const [position, setPosition] = React.useState({ x, y });
  const actions = React.useMemo(
    () => quickActions(task, targets),
    [targets, task],
  );
  const opensLeft = position.x + 480 > window.innerWidth;

  React.useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const rect = menu.getBoundingClientRect();
    const gutter = 8;
    setPosition({
      x: Math.max(gutter, Math.min(x, window.innerWidth - rect.width - gutter)),
      y: Math.max(gutter, Math.min(y, window.innerHeight - rect.height - gutter)),
    });
    enabledItems(menu)[0]?.focus();
  }, [x, y]);

  React.useLayoutEffect(() => {
    const panel = submenu?.kind === 'move' ? submenuRef.current : priorityRef.current;
    const trigger = submenu?.kind === 'move' ? submenuTriggerRef.current : priorityTriggerRef.current;
    if (!submenu || !panel || !trigger || window.innerWidth <= 720) return;
    const rect = panel.getBoundingClientRect();
    const triggerRect = trigger.getBoundingClientRect();
    const gutter = 8;
    const rootRect = menuRef.current?.getBoundingClientRect();
    const right = (rootRect?.right ?? triggerRect.right) + 5;
    const left = (rootRect?.left ?? triggerRect.left) - rect.width - 5;
    const desiredX = right + rect.width <= window.innerWidth - gutter ? right : left;
    const desiredY = triggerRect.top - 7;
    setSubmenuPosition({
      x: Math.max(gutter, Math.min(desiredX, window.innerWidth - rect.width - gutter)),
      y: Math.max(gutter, Math.min(desiredY, window.innerHeight - rect.height - gutter)),
    });
  }, [columns.length, position.x, position.y, submenu]);

  React.useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!menuRef.current?.contains(target) && !submenuRef.current?.contains(target) && !priorityRef.current?.contains(target)) {
        onClose(false);
      }
    };
    const close = () => onClose(false);
    const closeOnOutsideWheel = (event: WheelEvent) => {
      const target = event.target as Node | null;
      if (!menuRef.current?.contains(target) && !submenuRef.current?.contains(target) && !priorityRef.current?.contains(target)) {
        close();
      }
    };
    window.addEventListener('pointerdown', closeOutside, true);
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    window.addEventListener('wheel', closeOnOutsideWheel, { passive: true });
    return () => {
      window.removeEventListener('pointerdown', closeOutside, true);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
      window.removeEventListener('wheel', closeOnOutsideWheel);
    };
  }, [onClose]);

  React.useLayoutEffect(() => {
    const panel = submenu?.kind === 'move' ? submenuRef.current : priorityRef.current;
    if (submenu?.focusFirst && panel) enabledItems(panel)[0]?.focus();
  }, [submenu]);

  const focusIsInSubmenu = () => Boolean(
    submenuRef.current?.contains(document.activeElement)
    || priorityRef.current?.contains(document.activeElement),
  );
  const canHoverSubmenus = () => window.matchMedia('(min-width: 721px) and (hover: hover)').matches;
  const activate = (event: React.MouseEvent, action: () => void) => {
    event.stopPropagation();
    // A dialog opened by the action must not capture focus from a mounted menu.
    flushSync(() => onClose(false));
    action();
  };
  const runMove = (event: React.MouseEvent, columnId: string) => {
    activate(event, () => onMove(columnId));
  };
  const openSubmenu = (kind: 'move' | 'priority', focusFirst = false) => {
    if (kind === 'move' && columns.length === 0) return;
    setSubmenu({ kind, focusFirst: focusFirst || focusIsInSubmenu() });
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    event.stopPropagation();
    const menu = (event.target as HTMLElement).closest<HTMLElement>('[role="menu"]');
    if (event.key === 'Tab') {
      event.preventDefault();
      onClose(true);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onClose(true);
    } else if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && menu) {
      event.preventDefault();
      focusRelative(menu, event.target as Element, event.key === 'ArrowDown' ? 1 : -1);
      if (menu === menuRef.current) setSubmenu(null);
    } else if ((event.key === 'Home' || event.key === 'End') && menu) {
      event.preventDefault();
      if (menu === menuRef.current) setSubmenu(null);
      const items = enabledItems(menu);
      (event.key === 'Home' ? items[0] : items.at(-1))?.focus();
    } else if (event.key === 'ArrowRight' && event.target === priorityTriggerRef.current) {
      event.preventDefault();
      openSubmenu('priority', true);
    } else if (event.key === 'ArrowLeft' && priorityRef.current?.contains(event.target as Node)) {
      event.preventDefault();
      setSubmenu(null);
      priorityTriggerRef.current?.focus();
    } else if (event.key === 'ArrowRight' && event.target === submenuTriggerRef.current) {
      event.preventDefault();
      openSubmenu('move', true);
    } else if (event.key === 'ArrowLeft' && submenuRef.current?.contains(event.target as Node)) {
      event.preventDefault();
      setSubmenu(null);
      submenuTriggerRef.current?.focus();
    }
  };

  const prioritySubmenu = priorityOpen && onSetPriority && (
    <div
      ref={priorityRef}
      className="task-context-menu task-context-menu__submenu task-context-menu__submenu--portal"
      role="menu"
      aria-label="设置优先级"
      style={window.innerWidth > 720 ? { left: submenuPosition.x, top: submenuPosition.y } : undefined}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      {([1, 2, 3, 4, null] as TaskPriority[]).map((priority) => (
        <button
          type="button"
          role="menuitem"
          key={priority ?? 0}
          disabled={task.priority === priority && !allowCurrentPriority}
          onClick={(event) => activate(event, () => onSetPriority(priority))}
        >
          <span className={`priority-dot priority-dot--${priority ?? 0}`} />
          <span>{priority === null ? '清除优先级' : `!${priority} ${['最高', '高', '中', '低'][priority - 1]}`}</span>
          <kbd>{priority ?? 0}</kbd>
        </button>
      ))}
    </div>
  );

  const moveSubmenu = submenuOpen && columns.length > 0 && (
    <div
      ref={submenuRef}
      className="task-context-menu task-context-menu__submenu task-context-menu__submenu--portal"
      role="menu"
      aria-label="移动到列"
      style={window.innerWidth > 720 ? { left: submenuPosition.x, top: submenuPosition.y } : undefined}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      {columns.map((column) => {
        const stateAlreadyMatches = task.completed === column.done;
        const current = task.column === column.id && stateAlreadyMatches;
        return (
          <button
            type="button"
            role="menuitem"
            key={column.id}
            disabled={current}
            onClick={(event) => runMove(event, column.id)}
          >
            <span className={`column-dot column-dot--${column.done ? 'done' : 'active'}`} />
            <span>{column.name}</span>
            {current && <span className="task-context-menu__current">当前</span>}
          </button>
        );
      })}
    </div>
  );

  const menuPortal = createPortal(
    <div
      ref={menuRef}
      className={`task-context-menu${opensLeft ? ' opens-left' : ''}`}
      role="menu"
      aria-label={`“${plainInlineMarkdown(task.body)}”的任务菜单`}
      style={{ left: position.x, top: position.y }}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <span className="task-context-menu__task" aria-hidden="true">{plainInlineMarkdown(task.body)}</span>
      {onStartUnclassified && <button type="button" role="menuitem" onClick={(event) => activate(event, onStartUnclassified)}>
        <Icon name="arrow" /><span>开始处理</span>
      </button>}
      {task.column === null && !task.completed && columns.length > 0 && !targets.doing && (
        <p className="task-context-menu__hint">此来源没有可用的推进列，不能开始处理；不会覆盖已有列。</p>
      )}
      {actions.map((action) => (
        <button
          key={action.column.id}
          type="button"
          role="menuitem"
          onPointerEnter={(event) => {
            if (!canHoverSubmenus()) return;
            if (focusIsInSubmenu()) event.currentTarget.focus();
            setSubmenu(null);
          }}
          onClick={(event) => runMove(event, action.column.id)}
        >
          <Icon name={action.icon} />
          <span>{action.label}</span>
        </button>
      ))}
      {actions.length > 0 && <span className="task-context-menu__rule" role="separator" />}
      {onSetPriority && (
        <>
          <button
            ref={priorityTriggerRef}
            type="button"
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={priorityOpen}
            onClick={() => {
              if (priorityOpen) setSubmenu(null);
              else openSubmenu('priority', true);
            }}
          >
            <Icon name="grid" /><span>优先级…</span><Icon name="chevron-right" className="task-context-menu__chevron" />
          </button>
          {window.innerWidth <= 720 && prioritySubmenu}
        </>
      )}
      <button
        ref={submenuTriggerRef}
        type="button"
        role="menuitem"
        disabled={columns.length === 0}
        aria-haspopup={columns.length > 0 ? 'menu' : undefined}
        aria-expanded={submenuOpen}
        onClick={() => openSubmenu('move', true)}
        onPointerEnter={() => {
          // Reflowing narrow sheets can put this trigger under a stationary pointer.
          if (canHoverSubmenus()) openSubmenu('move');
        }}
      >
        <Icon name="move" />
        <span>移动到…</span>
        <Icon name="chevron-right" className="task-context-menu__chevron" />
      </button>
      {columns.length === 0 && <>
        <p className="task-context-menu__hint">当前来源未定义列，无法移动任务。</p>
        {onConfigureColumns && <button type="button" role="menuitem" onClick={(event) => activate(event, onConfigureColumns)}>
          <Icon name="grid" /><span>配置任务列…</span>
        </button>}
      </>}
      {window.innerWidth <= 720 && moveSubmenu}
    </div>,
    document.body,
  );

  return (
    <>
      {menuPortal}
      {window.innerWidth > 720 && submenu && createPortal(
        submenu.kind === 'move' ? moveSubmenu : prioritySubmenu,
        document.body,
      )}
    </>
  );
}
