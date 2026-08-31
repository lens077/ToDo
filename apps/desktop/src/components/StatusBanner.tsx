import type { AppBanner } from '../hooks/useDesktop';
import { Icon } from './Icon';

interface StatusBannerProps {
  banner: AppBanner;
  onDismiss: () => void;
  onRetry: () => void;
}

export function StatusBanner({ banner, onDismiss, onRetry }: StatusBannerProps) {
  return (
    <aside className={`status-banner status-banner--${banner.tone}`} role={banner.tone === 'error' ? 'alert' : 'status'}>
      <Icon name="alert" className="status-banner__icon" />
      <div>
        <strong>{banner.title}</strong>
        <p>{banner.detail}</p>
      </div>
      <button className="status-banner__retry" type="button" onClick={onRetry}>重新载入</button>
      <button className="icon-action status-banner__close" type="button" onClick={onDismiss} aria-label="关闭提示">
        <Icon name="close" />
      </button>
    </aside>
  );
}
