import * as React from 'react';
import {
  PROVIDER_MODELS,
  readProviderSettings,
  writeProviderSettings,
  type ModelProvider,
} from '../lib/providerSettings';

interface ProviderSettingsProps {
  onClose: () => void;
}

export function ProviderSettings({ onClose }: ProviderSettingsProps) {
  const [draft, setDraft] = React.useState(readProviderSettings);
  const [catalogMessage, setCatalogMessage] = React.useState<string | null>(null);
  const [saveError, setSaveError] = React.useState<string | null>(null);
  const modelOptions = [...new Set([...PROVIDER_MODELS[draft.provider], ...draft.custom_models])];

  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  const changeProvider = (provider: ModelProvider) => {
    setDraft((current) => ({
      ...current,
      provider,
      model: PROVIDER_MODELS[current.provider].includes(current.model)
        ? PROVIDER_MODELS[provider][0]
        : current.model,
    }));
    setCatalogMessage(null);
  };

  const addCurrentModel = () => {
    const model = draft.model.trim();
    if (!model) {
      setCatalogMessage('先输入要加入目录的模型 ID。');
      return;
    }
    if (modelOptions.includes(model)) {
      setCatalogMessage('这个模型已经在当前目录中。');
      return;
    }
    setDraft((current) => ({
      ...current,
      model,
      custom_models: [...current.custom_models, model],
    }));
    setCatalogMessage(`已把 ${model} 加入本地目录。`);
  };

  const save = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSaveError(null);
    try {
      writeProviderSettings({ ...draft, model: draft.model.trim() });
      onClose();
    } catch {
      setSaveError('浏览器拒绝写入本地配置，请检查站点存储权限。');
    }
  };

  return (
    <div className="provider-dialog" role="presentation">
      <section
        className="provider-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="provider-settings-title"
      >
        <header className="provider-panel__header">
          <div>
            <h2 id="provider-settings-title">模型提供方设置</h2>
          </div>
          <span className="provider-panel__local">仅存于此设备</span>
        </header>

        <form className="provider-form" onSubmit={save}>
          <div className="provider-field-grid">
            <label className="provider-field">
              <span>提供方</span>
              <select
                autoFocus
                value={draft.provider}
                onChange={(event) => changeProvider(event.target.value as ModelProvider)}
              >
                <option value="anthropic">Anthropic</option>
                <option value="openai">OpenAI</option>
              </select>
            </label>

            <label className="provider-field">
              <span>API 密钥</span>
              <input
                type="password"
                value={draft.api_key}
                onChange={(event) => setDraft((current) => ({ ...current, api_key: event.target.value }))}
                placeholder="输入 API 密钥，或留空使用环境认证"
                autoComplete="new-password"
                spellCheck={false}
              />
            </label>
          </div>

          <details className="provider-custom-settings">
            <summary>自定义设置</summary>
            <label className="provider-field">
              <span>API 地址</span>
              <input
                type="url"
                value={draft.base_url}
                onChange={(event) => setDraft((current) => ({ ...current, base_url: event.target.value }))}
                placeholder="提供方默认"
                inputMode="url"
                spellCheck={false}
              />
            </label>
          </details>

          <section className="provider-catalog" aria-labelledby="provider-catalog-title">
            <header className="provider-catalog__header">
              <div>
                <h3 id="provider-catalog-title">模型目录</h3>
              </div>
              <button
                className="text-action"
                type="button"
                onClick={() =>
                  setCatalogMessage(
                    `已载入 ${PROVIDER_MODELS[draft.provider].length} 个内置模型；远程目录将在后端接线后启用。`,
                  )
                }
              >
                获取可用模型
              </button>
            </header>

            <label className="provider-field provider-model-field">
              <span>当前模型</span>
              <input
                type="text"
                list="provider-model-options"
                value={draft.model}
                onChange={(event) => setDraft((current) => ({ ...current, model: event.target.value }))}
                placeholder="输入或选择模型 ID"
                autoComplete="off"
                spellCheck={false}
              />
              <datalist id="provider-model-options">
                {modelOptions.map((model) => (
                  <option value={model} key={model} />
                ))}
              </datalist>
            </label>

            <div className="provider-catalog__hint">
              <strong>目录说明</strong>
              <p>模型选择器中将不显示任何模型；目录外 ID 仍可直接发送。</p>
            </div>

            {draft.custom_models.length > 0 && (
              <div className="provider-custom-models" aria-label="自定义模型">
                {draft.custom_models.map((model) => (
                  <span key={model}>{model}</span>
                ))}
              </div>
            )}

            <div className="provider-catalog__actions">
              <button className="text-action" type="button" onClick={addCurrentModel}>
                添加模型
              </button>
              {catalogMessage && <span role="status">{catalogMessage}</span>}
            </div>
          </section>

          {saveError && <p className="provider-save-error" role="alert">{saveError}</p>}

          <footer className="provider-panel__actions">
            <button className="text-action" type="button" onClick={onClose}>
              取消
            </button>
            <button className="text-action text-action--primary" type="submit" disabled={!draft.model.trim()}>
              保存
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
