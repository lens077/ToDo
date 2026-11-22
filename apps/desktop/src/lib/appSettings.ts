import * as React from 'react';
import { isMainView, type MainView } from './views';

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
 * How 「打开会话」 on an AI card opens the host's link.
 *
 * - `reuse`: one window owned by ToDo (a named tab on the web); opening
 *   another session navigates it instead of adding a tab.
 * - `new-tab`: the system browser, a new tab every time.
 */
export type OpenSessionMode = 'reuse' | 'new-tab' | 'reuse-existing';

export const OPEN_SESSION_MODES: ReadonlyArray<{ value: OpenSessionMode; label: string; note: string }> = [
  { value: 'reuse', label: '复用同一窗口', note: '在 ToDo 自带的窗口里打开 DSH，再次打开时切换会话，不新开标签页。' },
  { value: 'new-tab', label: '每次新开标签页', note: '用系统默认浏览器打开。' },
  { value: 'reuse-existing', label: '复用已打开的 DSH 标签页', note: 'Firefox 扩展会定位已打开的 DSH 标签页；扩展不可用时回退为复用同一窗口。' },
];

/**
 * Which views the narrow-screen tab bar shows, in order. Device-local: the
 * mobile bar is a copy consumer, so this never enters the sync server.
 */
export interface MobileTabBarPreference {
  order: MainView[];
  version: 1;
}

/**
 * The narrow-screen tab bar's shape.
 *
 * - `apple`: a floating rounded bar with a hairline gap to every screen edge,
 *   translucent over the content behind it. The default.
 * - `classic`: the original full-width bar pinned to the bottom edge; its
 *   panel stops above the system navigation area on edge-to-edge devices.
 */
export type MobileTabStyle = 'apple' | 'classic';

export const MOBILE_TAB_STYLES: ReadonlyArray<{ value: MobileTabStyle; label: string; note: string }> = [
  { value: 'apple', label: 'Apple 风格', note: '悬浮圆角栏，四周留出缝隙，半透明毛玻璃。' },
  { value: 'classic', label: '经典', note: '贴底满宽的传统导航栏。' },
];

export const MOBILE_TAB_MIN = 2;
export const MOBILE_TAB_MAX = 6;

/**
 * How 「另存为 HTML / Markdown」 names the wrapper directory and file.
 *
 * - `product-suffix`: 「标题 - Sumery Virgo」 (default) — exports are
 *   recognizable at a glance.
 * - `plain`: title only, no affixes.
 * - `custom`: the user's own prefix and/or suffix; both empty ≙ `plain`.
 */
export type ExportNamingMode = 'product-suffix' | 'plain' | 'custom';

export const EXPORT_NAMING_MODES: ReadonlyArray<{ value: ExportNamingMode; label: string; note: string }> = [
  { value: 'product-suffix', label: '带产品后缀', note: '「标题 - Sumery Virgo」，导出的目录与文件一眼认出出处。' },
  { value: 'plain', label: '仅标题', note: '不加任何前后缀。' },
  { value: 'custom', label: '自定义', note: '用下面的前缀 / 后缀，可只用其一；都留空等同「仅标题」。' },
];

function isExportNamingMode(value: unknown): value is ExportNamingMode {
  return value === 'product-suffix' || value === 'plain' || value === 'custom';
}

export const defaultMobileTabBar: MobileTabBarPreference = {
  order: ['goal', 'reading', 'overview'],
  version: 1,
};

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
  openSession: OpenSessionMode;
  /**
   * Send anonymous interface usage (view and action names, never task
   * content) to the self-hosted Umami. On by default, disclosed in README and
   * the settings dialog; see lib/analytics.ts for exactly what is sent.
   */
  usageAnalytics: boolean;
  /**
   * Entering the reading view reads the clipboard once to spot copied http(s)
   * links. Off means no clipboard read happens at all: auto-discovery of
   * copied article links stops, manual paste into the reading input remains.
   */
  clipboardImport: boolean;
  /** 图片预览：点击图片外区域关闭（关闭后仅 ✕ / Esc 关闭，缩放手势不会误触）。 */
  lightboxOutsideClose: boolean;
  /**
   * Naming of the reader 「另存为」 wrapper directory and file. Only affects
   * exports made after the change; illegal characters are filtered by the
   * Rust side either way.
   */
  exportNaming: ExportNamingMode;
  /** Custom affixes, applied only when `exportNaming` is `custom`. */
  exportPrefix: string;
  exportSuffix: string;
  mobileTabBar: MobileTabBarPreference;
  /** Narrow-screen tab bar shape; see `MobileTabStyle`. */
  mobileTabStyle: MobileTabStyle;
}

export const APP_SETTINGS_STORAGE_KEY = 'todo.app-settings.v1';

export const defaultAppSettings: AppSettings = {
  lightboxOutsideClose: true,
  confirmCompletion: true,
  density: 'normal',
  openSession: 'reuse',
  usageAnalytics: true,
  clipboardImport: true,
  exportNaming: 'product-suffix',
  exportPrefix: '',
  exportSuffix: '',
  mobileTabBar: defaultMobileTabBar,
  mobileTabStyle: 'apple',
};

function isDensity(value: unknown): value is LayoutDensity {
  return value === 'normal' || value === 'medium' || value === 'efficient';
}

function isOpenSessionMode(value: unknown): value is OpenSessionMode {
  return value === 'reuse' || value === 'new-tab' || value === 'reuse-existing';
}

function isMobileTabStyle(value: unknown): value is MobileTabStyle {
  return value === 'apple' || value === 'classic';
}

function normalizeMobileTabBar(value: unknown): MobileTabBarPreference {
  const raw = value && typeof value === 'object' ? (value as { order?: unknown }) : {};
  const seen = new Set<MainView>();
  const order: MainView[] = [];
  if (Array.isArray(raw.order)) {
    for (const id of raw.order) {
      // Unknown ids and repeats are dropped silently; only the first six stick.
      // 随记视图移出 Tab 栏（A′）：入口收敛到收件箱工具行「随记分拣」。
      if (id === 'capture') continue;
      if (!isMainView(id) || seen.has(id)) continue;
      seen.add(id);
      order.push(id);
      if (order.length === MOBILE_TAB_MAX) break;
    }
  }
  // Fewer than two views is not a usable bar: fall back rather than prompt.
  if (order.length < MOBILE_TAB_MIN) return { order: [...defaultMobileTabBar.order], version: 1 };
  return { order, version: 1 };
}

function normalize(value: unknown): AppSettings {
  const raw = value && typeof value === 'object' ? (value as Partial<AppSettings>) : {};
  return {
    confirmCompletion: typeof raw.confirmCompletion === 'boolean' ? raw.confirmCompletion : defaultAppSettings.confirmCompletion,
    density: isDensity(raw.density) ? raw.density : defaultAppSettings.density,
    openSession: isOpenSessionMode(raw.openSession) ? raw.openSession : defaultAppSettings.openSession,
    usageAnalytics: typeof raw.usageAnalytics === 'boolean' ? raw.usageAnalytics : defaultAppSettings.usageAnalytics,
    clipboardImport: typeof raw.clipboardImport === 'boolean' ? raw.clipboardImport : defaultAppSettings.clipboardImport,
    lightboxOutsideClose: typeof raw.lightboxOutsideClose === 'boolean' ? raw.lightboxOutsideClose : defaultAppSettings.lightboxOutsideClose,
    exportNaming: isExportNamingMode(raw.exportNaming) ? raw.exportNaming : defaultAppSettings.exportNaming,
    exportPrefix: typeof raw.exportPrefix === 'string' ? raw.exportPrefix : defaultAppSettings.exportPrefix,
    exportSuffix: typeof raw.exportSuffix === 'string' ? raw.exportSuffix : defaultAppSettings.exportSuffix,
    mobileTabBar: normalizeMobileTabBar(raw.mobileTabBar),
    mobileTabStyle: isMobileTabStyle(raw.mobileTabStyle) ? raw.mobileTabStyle : defaultAppSettings.mobileTabStyle,
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

/** The settings as they stand now, for code that acts on a click rather than renders. */
export function currentAppSettings(): AppSettings {
  return snapshot();
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

/**
 * Write the normalized form back when stored JSON is out of shape (unknown
 * view ids, repeats, a bar below the minimum). Runs once per page, before the
 * first subscription, so it never fights a live write.
 */
function repairStoredSettings(): void {
  if (typeof window === 'undefined') return;
  try {
    const stored = window.localStorage.getItem(APP_SETTINGS_STORAGE_KEY);
    if (!stored) return;
    const serialized = JSON.stringify(normalize(JSON.parse(stored) as unknown));
    if (serialized !== stored) window.localStorage.setItem(APP_SETTINGS_STORAGE_KEY, serialized);
  } catch {
    // Unreadable storage: the defaults already apply.
  }
}

function subscribe(listener: () => void): () => void {
  if (listeners.size === 0) repairStoredSettings();
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Live view of the settings; every subscriber re-renders on a write. */
export function useAppSettings(): [AppSettings, (patch: Partial<AppSettings>) => void] {
  const settings = React.useSyncExternalStore(subscribe, snapshot, () => defaultAppSettings);
  return [settings, writeAppSettings];
}
