import * as React from 'react';
import { useAppSettings } from '../lib/appSettings';
import { readProviderSettings } from '../lib/providerSettings';
import { Icon } from './Icon';

interface SettingsDialogProps {
  onClose: () => void;
  onOpenProviderSettings: () => void;
}

export function SettingsDialog({ onClose, onOpenProviderSettings }: SettingsDialogProps) {
  const [settings, update] = useAppSettings();
  const provider = readProviderSettings();

  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  return (
    <div
      className="provider-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="settings-panel" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header className="settings-panel__header">
          <h2 id="settings-title">设置</h2>
          <button type="button" className="icon-action settings-panel__close" aria-label="关闭设置" onClick={onClose}>
            <Icon name="close" />
          </button>
        </header>

        <div className="settings-body">
          <section className="settings-group" aria-labelledby="settings-safety">
            <h3 id="settings-safety">操作确认</h3>
            <label className="settings-row">
              <span className="settings-row__copy">
                <strong>勾选完成前先确认</strong>
                <span>点卡片上的勾选框时弹出确认，回车完成、Esc 取消，避免误点直接写入文件。严格模式下始终需要填写结局说明，不受此项影响。</span>
              </span>
              <span className="switch">
                <input
                  type="checkbox"
                  role="switch"
                  checked={settings.confirmCompletion}
                  aria-label="勾选完成前先确认"
                  onChange={(event) => update({ confirmCompletion: event.target.checked })}
                />
                <span className="switch__track" aria-hidden="true" />
              </span>
            </label>
          </section>

          <section className="settings-group" aria-labelledby="settings-model">
            <h3 id="settings-model">语义对账</h3>
            <div className="settings-row">
              <span className="settings-row__copy">
                <strong>模型提供方</strong>
                <span>
                  {provider.api_key ? `${provider.provider} · ${provider.model}` : '尚未配置密钥；语义对账保持关闭。'}
                </span>
              </span>
              <button type="button" className="text-action" onClick={onOpenProviderSettings}>
                配置模型…
              </button>
            </div>
          </section>

          <p className="settings-footnote">这些偏好只保存在本机，不会写入任何项目文件。</p>
        </div>
      </section>
    </div>
  );
}
