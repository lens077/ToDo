import * as React from 'react';
import { EXPORT_NAMING_MODES, LAYOUT_DENSITIES, OPEN_SESSION_MODES, defaultAppSettings, useAppSettings } from '../lib/appSettings';
import { readProviderSettings } from '../lib/providerSettings';
import { Icon } from './Icon';
import { MobileTabBarEditor } from './MobileTabBarEditor';
import { navItems, type MainView } from './Sidebar';

interface SettingsDialogProps {
  onClose: () => void;
  onOpenProviderSettings: () => void;
  /** Onboarding-spec §7.1: the wizard is opt-in and only reachable from here. */
  onOpenWizard?: () => void;
  /** Narrow screens only: every view that did not fit in the tab bar is listed here. */
  activeView?: MainView;
  onChangeView?: (view: MainView) => void;
  /** Open scrolled to the tab editor, for the long-press entry on the bar. */
  initialSection?: 'tabs' | null;
}

export function SettingsDialog({ onClose, onOpenProviderSettings, onOpenWizard, activeView, onChangeView, initialSection }: SettingsDialogProps) {
  const [settings, update] = useAppSettings();
  // 随记迁移（A′）：inbox.md 一次性并入阅读库；命令只在桌面 Tauri 运行时存在。
  const [inboxMigration, setInboxMigration] = React.useState<{ running: boolean; report: string | null }>({ running: false, report: null });
  const tauriRuntime = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  const migrateInbox = async () => {
    if (inboxMigration.running) return;
    setInboxMigration({ running: true, report: null });
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const report = await invoke<{ migrated: number; archived_to: string }>('capture_migrate_inbox');
      setInboxMigration({
        running: false,
        report: report.migrated > 0
          ? `已迁移 ${report.migrated} 条随记到收件箱，原文件改名 ${report.archived_to.split('/').pop()}`
          : '没有需要迁移的随记（inbox.md 不存在或为空）',
      });
    } catch (cause) {
      setInboxMigration({ running: false, report: `迁移失败：${String(cause)}` });
    }
  };

  const provider = readProviderSettings();
  const panelRef = React.useRef<HTMLElement>(null);
  // 窄屏下设置占的是工作区那一行，底部导航仍可操作，所以不再是模态浮层。
  const [compact, setCompact] = React.useState(() => window.matchMedia('(max-width: 719px)').matches);

  React.useEffect(() => {
    const query = window.matchMedia('(max-width: 719px)');
    const sync = () => setCompact(query.matches);
    sync();
    query.addEventListener('change', sync);
    return () => query.removeEventListener('change', sync);
  }, []);

  React.useEffect(() => {
    if (initialSection !== 'tabs') return;
    panelRef.current?.querySelector('.settings-tabs')?.scrollIntoView({ block: 'start' });
  }, [initialSection]);

  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  return (
    <div
      className="provider-dialog settings-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="settings-panel" role="dialog" aria-modal={compact ? undefined : true} aria-labelledby="settings-title" ref={panelRef}>
        <header className="settings-panel__header">
          <h2 id="settings-title">设置</h2>
          <button type="button" className="icon-action settings-panel__close" aria-label="关闭设置" onClick={onClose}>
            <Icon name="close" />
          </button>
        </header>

        <div className="settings-body">
          {onOpenWizard && (
            <section className="settings-group" aria-labelledby="settings-onboarding">
              <h3 id="settings-onboarding">首次使用？</h3>
              <div className="settings-row">
                <span className="settings-row__copy">
                  <strong>打开引导向导</strong>
                  <span>用 5 步走一遍项目、扫描、agent 接入与对账。没有开机强推，随时可以跳过，也可以以后从这里再次打开。</span>
                </span>
                <button type="button" className="line-action" onClick={onOpenWizard}>
                  打开引导向导
                </button>
              </div>
            </section>
          )}

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

          <section className="settings-group" aria-labelledby="settings-clipboard">
            <h3 id="settings-clipboard">剪贴板</h3>
            <label className="settings-row">
              <span className="settings-row__copy">
                <strong>读取剪贴板识别链接</strong>
                <span>
                  进入阅读视图时会读取一次剪贴板，只识别其中的 http(s) 链接，用来询问是否导入阅读库。复制的其他文字不会被保存，也不会上传。
                  关闭后不再自动读取剪贴板：复制链接后自动发现并导入文章的功能会失效，需要在阅读视图里手动粘贴链接。
                </span>
              </span>
              <span className="switch">
                <input
                  type="checkbox"
                  role="switch"
                  checked={settings.clipboardImport}
                  aria-label="读取剪贴板识别链接"
                  onChange={(event) => update({ clipboardImport: event.target.checked })}
                />
                <span className="switch__track" aria-hidden="true" />
              </span>
            </label>
          </section>

          <section className="settings-group" aria-labelledby="settings-export-naming">
            <h3 id="settings-export-naming">另存为命名</h3>
            <div className="settings-row settings-row--stack">
              <span className="settings-row__copy">
                <strong>导出目录与文件名</strong>
                <span>{EXPORT_NAMING_MODES.find((item) => item.value === settings.exportNaming)?.note}</span>
              </span>
              <div className="segmented" role="radiogroup" aria-label="另存为命名">
                {EXPORT_NAMING_MODES.map((item) => (
                  <label key={item.value} className={`segmented__option${settings.exportNaming === item.value ? ' is-active' : ''}`}>
                    <input
                      type="radio"
                      name="export-naming"
                      value={item.value}
                      checked={settings.exportNaming === item.value}
                      onChange={() => update({ exportNaming: item.value })}
                    />
                    <span>{item.label}</span>
                  </label>
                ))}
              </div>
              {settings.exportNaming === 'custom' && (
                <div className="settings-export-affixes" role="group" aria-label="自定义前后缀">
                  <label>
                    <span>前缀（加在最前）</span>
                    <input
                      type="text"
                      value={settings.exportPrefix}
                      placeholder="例如：SV · "
                      onChange={(event) => update({ exportPrefix: event.target.value })}
                    />
                  </label>
                  <label>
                    <span>后缀（加在最后）</span>
                    <input
                      type="text"
                      value={settings.exportSuffix}
                      placeholder="例如： (Sumery Virgo)"
                      onChange={(event) => update({ exportSuffix: event.target.value })}
                    />
                  </label>
                  <span className="settings-export-affixes__note">预览：{settings.exportPrefix || ''}文章标题{settings.exportSuffix || ''}</span>
                </div>
              )}
            </div>
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
                    <span>{item.label}</span>
                    {item.value === defaultAppSettings.openSession && <span className="segmented__default" aria-hidden="true">默认</span>}
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

          {tauriRuntime && (
          <section className="settings-group" aria-labelledby="settings-inbox-migration">
            <h3 id="settings-inbox-migration">随记迁移</h3>
            <div className="settings-row">
              <span className="settings-row__copy">
                <strong>把旧随记（inbox.md）并入收件箱</strong>
                <span>随记的事实源已迁入阅读库（与文章同库、随同步）。执行后 ~/.todo/inbox.md 里的每一条都会成为收件箱随记，原文件改名留档；重复执行是安全的。</span>
              </span>
              <button type="button" className="text-action" disabled={inboxMigration.running} onClick={() => void migrateInbox()}>
                {inboxMigration.running ? '迁移中…' : '立即迁移'}
              </button>
            </div>
            {inboxMigration.report && <p className="settings-row__copy" role="status">{inboxMigration.report}</p>}
          </section>
          )}
          <section className="settings-group" aria-labelledby="settings-lightbox">
            <h3 id="settings-lightbox">图片预览</h3>
            <label className="settings-row">
              <span className="settings-row__copy">
                <strong>点击图片外区域关闭</strong>
                <span>阅读页点开大图后，点图片外的深色区域即关闭预览。关闭此项后，预览支持双指捏合与双击缩放、拖动查看细节，只能用右上角 ✕ 或 Esc 关闭，缩放手势不会误触。</span>
              </span>
              <span className="switch">
                <input
                  type="checkbox"
                  role="switch"
                  checked={settings.lightboxOutsideClose}
                  aria-label="点击图片外区域关闭图片预览"
                  onChange={(event) => update({ lightboxOutsideClose: event.target.checked })}
                />
                <span className="switch__track" aria-hidden="true" />
              </span>
            </label>
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

          {onChangeView && (
            <MobileTabBarEditor
              order={settings.mobileTabBar.order}
              onChange={(order) => update({ mobileTabBar: { order, version: 1 } })}
              style={settings.mobileTabStyle}
              onStyleChange={(style) => update({ mobileTabStyle: style })}
            />
          )}

          {onChangeView && (
            <section className="settings-group settings-views" aria-labelledby="settings-views">
              <h3 id="settings-views">更多视图</h3>
              <span className="settings-views__hint">底部导航栏只放最常用的几项，其余视图从这里进入。</span>
              <div className="settings-views__grid">
                {navItems.filter((item) => !settings.mobileTabBar.order.includes(item.id)).map((item) => (
                  <button
                    type="button"
                    key={item.id}
                    className={activeView === item.id ? 'is-active' : ''}
                    aria-current={activeView === item.id ? 'page' : undefined}
                    onClick={() => {
                      onChangeView(item.id);
                      onClose();
                    }}
                  >
                    <Icon name={item.icon} />
                    <span>{item.label}</span>
                  </button>
                ))}
              </div>
            </section>
          )}

          <p className="settings-footnote">这些偏好只保存在本机，不会写入任何项目文件。</p>
        </div>
      </section>
    </div>
  );
}
