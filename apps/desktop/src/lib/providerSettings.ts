export type ModelProvider = 'anthropic' | 'openai';

export interface ProviderSettings {
  provider: ModelProvider;
  api_key: string;
  base_url: string;
  model: string;
  custom_models: string[];
}

export const PROVIDER_SETTINGS_STORAGE_KEY = 'todo.provider-settings.v1';

export const PROVIDER_MODELS: Record<ModelProvider, readonly string[]> = {
  anthropic: ['claude-opus-5', 'claude-fable-5'],
  openai: ['gpt-5.6-sol', 'gpt-5.6-terra'],
};

export function defaultProviderSettings(provider: ModelProvider = 'anthropic'): ProviderSettings {
  return {
    provider,
    api_key: '',
    base_url: '',
    model: PROVIDER_MODELS[provider][0],
    custom_models: [],
  };
}

function normalizeProvider(value: unknown): ModelProvider {
  return value === 'openai' ? 'openai' : 'anthropic';
}

function normalizeModelIds(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [...new Set(value.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean))];
}

function normalizeSettings(value: unknown): ProviderSettings {
  const raw = value && typeof value === 'object' ? (value as Partial<ProviderSettings>) : {};
  const provider = normalizeProvider(raw.provider);
  return {
    provider,
    api_key: typeof raw.api_key === 'string' ? raw.api_key : '',
    base_url: typeof raw.base_url === 'string' ? raw.base_url.trim() : '',
    model:
      typeof raw.model === 'string' && raw.model.trim()
        ? raw.model.trim()
        : PROVIDER_MODELS[provider][0],
    custom_models: normalizeModelIds(raw.custom_models),
  };
}

export function readProviderSettings(): ProviderSettings {
  if (typeof window === 'undefined') return defaultProviderSettings();
  try {
    const stored = window.localStorage.getItem(PROVIDER_SETTINGS_STORAGE_KEY);
    return stored ? normalizeSettings(JSON.parse(stored) as unknown) : defaultProviderSettings();
  } catch {
    return defaultProviderSettings();
  }
}

export function writeProviderSettings(settings: ProviderSettings): ProviderSettings {
  const normalized = normalizeSettings(settings);
  window.localStorage.setItem(PROVIDER_SETTINGS_STORAGE_KEY, JSON.stringify(normalized));
  return normalized;
}
