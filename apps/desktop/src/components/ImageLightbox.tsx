import * as React from 'react';
import { Icon } from './Icon';

/**
 * 正文图片全屏预览（A′ 验收迭代）：支持双指捏合缩放、双击切换 1x/2x、
 * 放大后拖动平移。关闭方式由设置项 `lightboxOutsideClose` 决定——默认
 * 「点击图片外区域关闭」；关闭该设置后只剩 ✕ / Esc，缩放手势不会误触。
 * 页面级缩放已被 viewport meta 禁用，这里的捏合是元素 transform，不冲突。
 */
export function ImageLightbox({
  src,
  outsideClose,
  onClose,
}: {
  src: string;
  outsideClose: boolean;
  onClose: () => void;
}) {
  const [transform, setTransform] = React.useState({ scale: 1, x: 0, y: 0 });
  const pointers = React.useRef(new Map<number, { x: number; y: number }>());
  const pinch = React.useRef<{ dist: number; scale: number } | null>(null);
  const drag = React.useRef<{ px: number; py: number; ox: number; oy: number } | null>(null);
  const lastTap = React.useRef<{ t: number; x: number; y: number } | null>(null);
  const imgRef = React.useRef<HTMLImageElement>(null);

  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  const apply = (next: { scale: number; x: number; y: number }) => {
    const scale = Math.min(6, Math.max(1, next.scale));
    if (scale === 1) {
      setTransform({ scale: 1, x: 0, y: 0 });
      return;
    }
    setTransform({ scale, x: next.x, y: next.y });
  };

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    // 不用 setPointerCapture：捕获会把后续 click 的 target 重定向到容器，
    // 让「点击图片本身」也被判成点击遮罩而误关闭。子元素事件冒泡到容器
    // 已经足够支撑双指与拖动。
    pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointers.current.size === 2) {
      const [a, b] = Array.from(pointers.current.values());
      pinch.current = { dist: Math.hypot(a.x - b.x, a.y - b.y), scale: transform.scale };
      drag.current = null;
    } else if (pointers.current.size === 1 && transform.scale > 1) {
      drag.current = { px: event.clientX, py: event.clientY, ox: transform.x, oy: transform.y };
    }
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!pointers.current.has(event.pointerId)) return;
    pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointers.current.size === 2 && pinch.current) {
      const [a, b] = Array.from(pointers.current.values());
      const dist = Math.hypot(a.x - b.x, a.y - b.y);
      const scale = Math.min(6, Math.max(1, pinch.current.scale * (dist / (pinch.current.dist || 1))));
      // 缩放以视口中心为锚（offset 清零），双指操作足够直觉。
      setTransform({ scale, x: 0, y: 0 });
      if (scale === 1) drag.current = null;
    } else if (pointers.current.size === 1 && drag.current) {
      setTransform({
        scale: transform.scale,
        x: drag.current.ox + (event.clientX - drag.current.px),
        y: drag.current.oy + (event.clientY - drag.current.py),
      });
    }
  };

  const onPointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    pointers.current.delete(event.pointerId);
    if (pointers.current.size < 2) pinch.current = null;
    if (pointers.current.size === 1 && transform.scale > 1) {
      const [p] = Array.from(pointers.current.values());
      drag.current = { px: p.x, py: p.y, ox: transform.x, oy: transform.y };
    } else if (pointers.current.size === 0) {
      drag.current = null;
      // 双击（触屏双 tap / 桌面双击的第二次 up）：1x ↔ 2x。
      const now = Date.now();
      const lt = lastTap.current;
      if (lt && now - lt.t < 320 && Math.hypot(event.clientX - lt.x, event.clientY - lt.y) < 32) {
        setTransform((current) => (current.scale > 1 ? { scale: 1, x: 0, y: 0 } : { scale: 2, x: 0, y: 0 }));
        lastTap.current = null;
      } else {
        lastTap.current = { t: now, x: event.clientX, y: event.clientY };
      }
    }
  };

  return (
    <div
      className="img-lightbox"
      role="dialog"
      aria-label="图片预览"
      data-outside-close={outsideClose ? 'on' : 'off'}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={(event) => pointers.current.delete(event.pointerId)}
      onClick={(event) => {
        // 只认点在遮罩上的点击；缩放手势（双指）不会派发 click，不冲突。
        if (outsideClose && event.target === event.currentTarget) onClose();
      }}
    >
      <img
        ref={imgRef}
        src={src}
        alt=""
        style={{ transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})` }}
        onClick={(event) => event.stopPropagation()}
      />
      <button type="button" className="img-lightbox__close" aria-label="关闭图片预览" onClick={onClose}>
        <Icon name="close" />
      </button>
      {transform.scale > 1 && (
        <button
          type="button"
          className="img-lightbox__reset"
          aria-label="恢复原始大小"
          onClick={() => setTransform({ scale: 1, x: 0, y: 0 })}
        >
          1:1
        </button>
      )}
    </div>
  );
}
