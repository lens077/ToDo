import * as React from 'react';

/**
 * How much room the interface gives each thing.
 *
 * - `normal`: the paper world as designed — heading, ribbon, roomy cards.
 * - `medium`: the same composition on a tighter grid; nothing removed.
 * - `efficient`: for people who live in the app all day. Ornament goes
 *   (ribbon, entrance rise, centred empties), every band and card packs
 *   closer, and the content column starts at the left edge instead of
 *   floating in margins.
 */
export type LayoutDensity = 'normal' | 'medium' | 'efficient';

export const LAYOUT_DENSITIES: ReadonlyArray<{ value: LayoutDensity; label: string; note: string }> = [
  { value: 'normal', label: '正常', note: '默认布局，留白与丝带齐全。' },
  { value: 'medium', label: '中等', note: '同样的构图，间距收紧一档。' },
  { value: 'efficient', label: '效率', note: '去掉装饰与居中留白，信息密度最高。' },
];

/**
 * Per-machine interface preferences. Nothing here is product data: it never
 * enters TODO.md and a cleared browser profile only restores the defaults.
 */
export interface AppSettings {
  /**
   * Ask before a tick marks a Project Task complete. On by default: a stray
   * click on a checkbox is a file write, and undoing it is another one.
   */
  confirmCompletion: boolean;
  density: LayoutDensity;
}

export const APP_SETTINGS_STORAGE_KEY = 'todo.app-settings.v1';

export const defaultAppSettings: AppSettings = {
  confirmCompletion: true,
  density: 'normal',
};

function isDensity(value: unknown): value is LayoutDensity {
  return value === 'normal' || value === 'medium' || value === 'efficient';
}

function normalize(value: unknown): AppSettings {
  const raw = value && typeof value === 'object' ? (value as Partial<AppSettings>) : {};
  return {
    confirmCompletion: typeof raw.confirmCompletion === 'boolean' ? raw.confirmCompletion : defaultAppSettings.confirmCompletion,
    density: isDensity(raw.density) ? raw.density : defaultAppSettings.density,
  };
}

export function readAppSettings(): AppSettings {
  if (typeof window === 'undefined') return defaultAppSettings;
  try {
    const stored = window.localStorage.getItem(APP_SETTINGS_STORAGE_KEY);
    return stored ? normalize(JSON.parse(stored) as unknown) : defaultAppSettings;
  } catch {
    return defaultAppSettings;
  }
}

const listeners = new Set<() => void>();
let current: AppSettings | null = null;

function snapshot(): AppSettings {
  if (!current) current = readAppSettings();
  return current;
}

export function writeAppSettings(patch: Partial<AppSettings>): AppSettings {
  const next = normalize({ ...snapshot(), ...patch });
  current = next;
  try {
    window.localStorage.setItem(APP_SETTINGS_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage unavailable: the setting still holds for this session.
  }
  listeners.forEach((listener) => listener());
  return next;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Live view of the settings; every subscriber re-renders on a write. */
export function useAppSettings(): [AppSettings, (patch: Partial<AppSettings>) => void] {
  const settings = React.useSyncExternalStore(subscribe, snapshot, () => defaultAppSettings);
  return [settings, writeAppSettings];
}
