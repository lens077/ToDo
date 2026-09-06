import * as React from 'react';

/** Keep long single-column drop targets reachable with a desktop pointer. */
export function useDragAutoScroll(active: boolean) {
  React.useEffect(() => {
    if (!active) return;
    let pointerY: number | null = null;
    let scrollable: HTMLElement | null = null;
    let frame = 0;
    let previous = 0;
    const track = (event: DragEvent) => {
      pointerY = event.clientY;
      let element = event.target instanceof HTMLElement ? event.target : null;
      scrollable = null;
      while (element) {
        const style = getComputedStyle(element);
        if (/(auto|scroll)/.test(style.overflowY) && element.scrollHeight > element.clientHeight) {
          scrollable = element;
          break;
        }
        element = element.parentElement;
      }
      scrollable ??= document.scrollingElement instanceof HTMLElement ? document.scrollingElement : null;
    };
    const tick = (now: number) => {
      if (pointerY !== null && scrollable) {
        const bounds = scrollable === document.scrollingElement
          ? { top: 0, bottom: window.innerHeight }
          : scrollable.getBoundingClientRect();
        const top = Math.max(0, bounds.top);
        const bottom = Math.min(window.innerHeight, bounds.bottom);
        const edge = 56;
        const speed = pointerY < top + edge
          ? -Math.min(1, (top + edge - pointerY) / edge)
          : pointerY > bottom - edge
            ? Math.min(1, (pointerY - bottom + edge) / edge)
            : 0;
        scrollable.scrollTop += speed * Math.min(now - previous || 16, 32) * 0.65;
      }
      previous = now;
      frame = requestAnimationFrame(tick);
    };
    const stop = () => { pointerY = null; };
    document.addEventListener('dragover', track);
    document.addEventListener('drop', stop);
    document.addEventListener('dragend', stop);
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('dragover', track);
      document.removeEventListener('drop', stop);
      document.removeEventListener('dragend', stop);
    };
  }, [active]);
}
