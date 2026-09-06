import * as React from 'react';
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
  const [submenuTop, setSubmenuTop] = React.useState(-7);
  const [position, setPosition] = React.useState({ x, y });
  const actions = React.useMemo(
    () => quickActions(task, targets),
    [targets, task],
  );

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
    const submenu = submenuRef.current;
    if (!submenuOpen || !submenu || window.innerWidth <= 720) return;
    const rect = submenu.getBoundingClientRect();
    const desiredTop = position.y - 7;
    const viewportTop = Math.max(
      8,
      Math.min(desiredTop, window.innerHeight - rect.height - 8),
    );
    setSubmenuTop(viewportTop - position.y);
  }, [columns.length, position.y, submenuOpen]);

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

  const opensLeft = position.x + 480 > window.innerWidth;

  return createPortal(
    <div
      ref={menuRef}
      className={`task-context-menu${opensLeft ? ' opens-left' : ''}`}
      role="menu"
      aria-label={`“${task.body}”的任务菜单`}
      style={{ left: position.x, top: position.y }}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <span className="task-context-menu__task" aria-hidden="true">{task.body}</span>
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
          {priorityOpen && (
            <div ref={priorityRef} className="task-context-menu task-context-menu__submenu task-context-menu__priority" role="menu" aria-label="设置优先级">
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
          )}
        </>
      )}
      <button
        ref={submenuTriggerRef}
        type="button"
        role="menuitem"
        aria-haspopup="menu"
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
      {submenuOpen && (
        <div
          ref={submenuRef}
          className="task-context-menu task-context-menu__submenu"
          role="menu"
          aria-label="移动到列"
          style={window.innerWidth > 720 ? { top: submenuTop } : undefined}
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
                <span
                  className={`column-dot column-dot--${column.done ? 'done' : 'active'}`}
                />
                <span>{column.name}</span>
                {current && <span className="task-context-menu__current">当前</span>}
              </button>
            );
          })}
        </div>
      )}
    </div>,
    document.body,
  );
}
