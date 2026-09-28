import * as React from 'react';
import { LAYOUT_DENSITIES, OPEN_SESSION_MODES, defaultAppSettings, useAppSettings } from '../lib/appSettings';
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

          <section className="settings-group" aria-labelledby="settings-density">
            <h3 id="settings-density">布局密度</h3>
            <div className="settings-row settings-row--stack">
              <span className="settings-row__copy">
                <strong>界面密度</strong>
                <span>{LAYOUT_DENSITIES.find((item) => item.value === settings.density)?.note}</span>
              </span>
              <div className="segmented" role="radiogroup" aria-label="界面密度">
                {LAYOUT_DENSITIES.map((item) => (
                  <label key={item.value} className={`segmented__option${settings.density === item.value ? ' is-active' : ''}`}>
                    <input
                      type="radio"
                      name="layout-density"
                      value={item.value}
                      checked={settings.density === item.value}
                      onChange={() => update({ density: item.value })}
                    />
                    <span>{item.label}</span>
                  </label>
                ))}
              </div>
            </div>
          </section>

          <section className="settings-group" aria-labelledby="settings-open-session">
            <h3 id="settings-open-session">打开会话</h3>
            <div className="settings-row settings-row--stack">
              <span className="settings-row__copy">
                <strong>AI 会话链接的打开方式</strong>
                <span>{OPEN_SESSION_MODES.find((item) => item.value === settings.openSession)?.note}</span>
              </span>
              <div className="segmented" role="radiogroup" aria-label="打开会话">
                {OPEN_SESSION_MODES.map((item) => (
                  <label key={item.value} className={`segmented__option${settings.openSession === item.value ? ' is-active' : ''}`}>
                    <input
                      type="radio"
                      name="open-session"
                      value={item.value}
                      checked={settings.openSession === item.value}
                      onChange={() => update({ openSession: item.value })}
                    />
                    <span>{item.label}{item.value === defaultAppSettings.openSession ? '（默认）' : ''}</span>
                  </label>
                ))}
              </div>
            </div>
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

          <section className="settings-group" aria-labelledby="settings-analytics">
            <h3 id="settings-analytics">使用统计</h3>
            <label className="settings-row">
              <span className="settings-row__copy">
                <strong>发送匿名使用统计</strong>
                <span>只发送打开了哪个视图、用了哪些功能，以及版本号、系统和屏幕尺寸，用来判断哪些界面最常用。不发送任务内容、文件路径或项目名，不设 Cookie、不记录身份。</span>
              </span>
              <span className="switch">
                <input
                  type="checkbox"
                  role="switch"
                  checked={settings.usageAnalytics}
                  aria-label="发送匿名使用统计"
                  onChange={(event) => update({ usageAnalytics: event.target.checked })}
                />
                <span className="switch__track" aria-hidden="true" />
              </span>
            </label>
          </section>

          <p className="settings-footnote">这些偏好只保存在本机，不会写入任何项目文件。</p>
        </div>
      </section>
    </div>
  );
}
