import * as React from 'react';
import { MOBILE_TAB_MAX, MOBILE_TAB_MIN } from '../lib/appSettings';
import type { MainView } from '../lib/views';
import { Icon } from './Icon';
import { navItems } from './Sidebar';

/** How long a chip must be held before it starts following the pointer. */
const DRAG_HOLD_MS = 300;

interface MobileTabBarEditorProps {
  order: MainView[];
  onChange: (order: MainView[]) => void;
}

function labelFor(id: MainView) {
  return navItems.find((item) => item.id === id)?.label ?? id;
}

function iconFor(id: MainView) {
  return navItems.find((item) => item.id === id)?.icon ?? 'grid';
}

/**
 * The narrow-screen tab bar editor: a live preview on top (drag to reorder,
 * − to remove), the remaining views below (+ to add). Every change is written
 * immediately; there is no save button.
 *
 * Reordering uses Pointer Events rather than HTML5 drag-and-drop, which
 * Android WebView handles poorly. Slot positions are captured when the drag
 * starts so the hit test does not chase chips as they animate into place.
 */
export function MobileTabBarEditor({ order, onChange }: MobileTabBarEditorProps) {
  const [draft, setDraft] = React.useState<MainView[]>(order);
  const draftRef = React.useRef<MainView[]>(order);
  const chipRefs = React.useRef(new Map<MainView, HTMLDivElement>());
  const slotsRef = React.useRef<Array<{ left: number; right: number; top: number; bottom: number }>>([]);
  const holdRef = React.useRef<{ id: MainView; x: number; y: number; timer: number } | null>(null);
  const dragRef = React.useRef<MainView | null>(null);
  const [dragging, setDragging] = React.useState<MainView | null>(null);

  React.useEffect(() => {
    if (dragRef.current) return;
    draftRef.current = order;
    setDraft(order);
  }, [order]);

  React.useEffect(() => () => {
    if (holdRef.current) window.clearTimeout(holdRef.current.timer);
    document.body.classList.remove('is-tab-dragging');
  }, []);

  const commit = (next: MainView[]) => {
    draftRef.current = next;
    setDraft(next);
    onChange(next);
  };

  const captureSlots = () => {
    slotsRef.current = draftRef.current.map((id) => {
      const rect = chipRefs.current.get(id)?.getBoundingClientRect();
      return rect
        ? { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom }
        : { left: 0, right: 0, top: 0, bottom: 0 };
    });
  };

  const animate = (before: Map<MainView, DOMRect>) => {
    requestAnimationFrame(() => {
      chipRefs.current.forEach((element, id) => {
        const previous = before.get(id);
        if (!previous) return;
        const rect = element.getBoundingClientRect();
        const dx = previous.left - rect.left;
        const dy = previous.top - rect.top;
        if (!dx && !dy) return;
        element.animate(
          [{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }],
          { duration: 180, easing: 'cubic-bezier(.2, .8, .2, 1)' },
        );
      });
    });
  };

  const measure = () => {
    const rects = new Map<MainView, DOMRect>();
    chipRefs.current.forEach((element, id) => rects.set(id, element.getBoundingClientRect()));
    return rects;
  };

  const slotAt = (x: number, y: number) => {
    const slots = slotsRef.current;
    const margin = 6;
    const inside = slots.findIndex((slot) =>
      x >= slot.left - margin && x <= slot.right + margin && y >= slot.top - margin && y <= slot.bottom + margin);
    if (inside >= 0) return inside;
    // Between/outside chips: pick the closest centre so a fast flick still lands.
    let best = -1;
    let bestDistance = Number.POSITIVE_INFINITY;
    slots.forEach((slot, index) => {
      const distance = Math.abs(x - (slot.left + slot.right) / 2);
      if (distance < bestDistance) {
        best = index;
        bestDistance = distance;
      }
    });
    return best;
  };

  const clearHold = () => {
    if (!holdRef.current) return;
    window.clearTimeout(holdRef.current.timer);
    holdRef.current = null;
  };

  const endDrag = () => {
    if (!dragRef.current) return;
    dragRef.current = null;
    setDragging(null);
    document.body.classList.remove('is-tab-dragging');
    onChange(draftRef.current);
  };

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>, id: MainView) => {
    if ((event.target as HTMLElement).closest('button')) return;
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    holdRef.current = {
      id,
      x: event.clientX,
      y: event.clientY,
      timer: window.setTimeout(() => {
        holdRef.current = null;
        dragRef.current = id;
        captureSlots();
        setDragging(id);
        document.body.classList.add('is-tab-dragging');
      }, DRAG_HOLD_MS),
    };
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const hold = holdRef.current;
    if (hold) {
      // A finger that moves before the hold expires is scrolling, not dragging.
      if (Math.abs(event.clientX - hold.x) > 8 || Math.abs(event.clientY - hold.y) > 8) clearHold();
      return;
    }
    const id = dragRef.current;
    if (!id) return;
    const slot = slotAt(event.clientX, event.clientY);
    if (slot < 0) return;
    const current = draftRef.current;
    const from = current.indexOf(id);
    if (from < 0 || from === slot) return;
    const before = measure();
    const next = [...current];
    next.splice(from, 1);
    next.splice(slot, 0, id);
    draftRef.current = next;
    setDraft(next);
    animate(before);
  };

  const onPointerUp = () => {
    clearHold();
    endDrag();
  };

  const remove = (id: MainView) => {
    if (draftRef.current.length <= MOBILE_TAB_MIN) return;
    commit(draftRef.current.filter((item) => item !== id));
  };

  const add = (id: MainView) => {
    if (draftRef.current.length >= MOBILE_TAB_MAX || draftRef.current.includes(id)) return;
    commit([...draftRef.current, id]);
  };

  const pool = navItems.filter((item) => !draft.includes(item.id));
  const clampNote = draft.length <= MOBILE_TAB_MIN
    ? `至少保留 ${MOBILE_TAB_MIN} 个视图`
    : draft.length >= MOBILE_TAB_MAX
      ? `最多 ${MOBILE_TAB_MAX} 个视图，其余收纳在「我的」`
      : '';

  return (
    <section className="settings-group settings-tabs" aria-labelledby="settings-tabs">
      <h3 id="settings-tabs">底部导航栏</h3>
      <span className="settings-tabs__hint">长按并拖动调整顺序；点 − 移除，点 ＋ 加入。</span>

      <div className="settings-tabs__preview" aria-label="当前底部导航">
        {draft.map((id) => (
          <div
            key={id}
            className={`tab-chip${dragging === id ? ' is-dragging' : ''}`}
            data-view={id}
            aria-label={`${labelFor(id)}，可拖动排序`}
            ref={(element) => {
              if (element) chipRefs.current.set(id, element);
              else chipRefs.current.delete(id);
            }}
            onPointerDown={(event) => onPointerDown(event, id)}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
          >
            <Icon name={iconFor(id)} />
            <span>{labelFor(id)}</span>
            <button
              type="button"
              className="tab-chip__remove"
              aria-label={`移除${labelFor(id)}`}
              disabled={draft.length <= MOBILE_TAB_MIN}
              onClick={() => remove(id)}
            >
              <Icon name="minus" />
            </button>
          </div>
        ))}
      </div>

      <span className="settings-tabs__clamp" role="status">{clampNote}</span>

      <div className="settings-tabs__pool">
        {pool.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-label={`加入${item.label}`}
            disabled={draft.length >= MOBILE_TAB_MAX}
            onClick={() => add(item.id)}
          >
            <Icon name="plus" />
            <span>{item.label}</span>
          </button>
        ))}
      </div>
    </section>
  );
}
