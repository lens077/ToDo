import * as React from 'react';

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
}

export const APP_SETTINGS_STORAGE_KEY = 'todo.app-settings.v1';

export const defaultAppSettings: AppSettings = {
  confirmCompletion: true,
};

function normalize(value: unknown): AppSettings {
  const raw = value && typeof value === 'object' ? (value as Partial<AppSettings>) : {};
  return {
    confirmCompletion: typeof raw.confirmCompletion === 'boolean' ? raw.confirmCompletion : defaultAppSettings.confirmCompletion,
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
