import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * 桌面阅读库 ↔ todo-sync 服务器的自动对账。
 *
 * 配置与网页端 sync-web 共用 `virgo-sync-config`。桌面侧把配置交给 Rust 的
 * 常驻线程：它保持 `/stream` 长连接，连上就先跑一次全量对账（离线期间写入的
 * 条目在这一步补推），之后每个 change 事件触发一次；断开则按 5s→60s 退避重试。
 */
export const SYNC_CONFIG_KEY = 'virgo-sync-config';

export interface SyncConfig {
  base: string;
  token: string;
}

export interface SyncReport {
  pulled: number;
  pushed: number;
  /** 已有一轮同步在跑，本轮直接跳过。 */
  skipped: boolean;
  /** 本轮在完成前被取消。 */
  cancelled: boolean;
  errors: string[];
}

export interface SyncProgress {
  stage: 'reconcile' | 'push' | 'pull' | 'done';
  done: number;
  total: number;
  label: string;
}

export type LinkState = 'idle' | 'connecting' | 'online' | 'offline';

export interface LinkStatus {
  state: LinkState;
  retry_in: number;
  error: string | null;
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function readSyncConfig(): SyncConfig | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(SYNC_CONFIG_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { base?: unknown; token?: unknown };
    const base = typeof parsed.base === 'string' ? parsed.base.trim() : '';
    const token = typeof parsed.token === 'string' ? parsed.token.trim() : '';
    return base && token ? { base, token } : null;
  } catch {
    return null;
  }
}

export function writeSyncConfig(config: SyncConfig): void {
  try {
    window.localStorage.setItem(SYNC_CONFIG_KEY, JSON.stringify({ base: config.base.trim(), token: config.token.trim() }));
  } catch {
    // 存储不可用时本次会话仍可用。
  }
}

export function clearSyncConfig(): void {
  try {
    window.localStorage.removeItem(SYNC_CONFIG_KEY);
  } catch {
    // 存储不可用时无需清理。
  }
}

/** 交给 Rust 常驻线程；重复调用会以新配置重启连接。 */
export async function startSyncWatcher(config: SyncConfig): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke('reading_sync_watch_start', { base: config.base.trim(), token: config.token.trim() });
}

export async function stopSyncWatcher(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke('reading_sync_watch_stop');
}

/** 取消正在进行的同步：当前这一篇处理完后停下，已完成的保留。 */
export async function cancelSync(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke('reading_sync_cancel');
}

export interface SyncEventHandlers {
  onLink?: (status: LinkStatus) => void;
  onStart?: () => void;
  onProgress?: (progress: SyncProgress) => void;
  onSynced?: (report: SyncReport) => void;
  onError?: (message: string) => void;
}

/** 订阅自动对账事件。返回取消订阅的函数。 */
export async function onSyncEvents(handlers: SyncEventHandlers): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  const unlisten = await Promise.all([
    handlers.onLink ? listen<LinkStatus>('reading://link', (event) => handlers.onLink?.(event.payload)) : null,
    handlers.onStart ? listen('reading://sync-start', () => handlers.onStart?.()) : null,
    handlers.onProgress
      ? listen<SyncProgress>('reading://sync-progress', (event) => handlers.onProgress?.(event.payload))
      : null,
    handlers.onSynced ? listen<SyncReport>('reading://synced', (event) => handlers.onSynced?.(event.payload)) : null,
    handlers.onError ? listen<string>('reading://sync-error', (event) => handlers.onError?.(event.payload)) : null,
  ]);
  return () => unlisten.forEach((stop) => stop?.());
}

/** 把一份报告压成界面上的单行摘要。同步不再本地解析，所以只有上下行两栏。 */
export function formatSyncReport(report: SyncReport): string {
  if (report.skipped) return '同步已在进行中';
  const parts = [`下行 ${report.pulled}`, `上行 ${report.pushed}`];
  if (report.cancelled) parts.push('已取消');
  if (report.errors.length > 0) parts.push(report.errors[0]);
  return parts.join(' · ');
}

function shorten(label: string, max = 22): string {
  return label.length > max ? `${label.slice(0, max)}…` : label;
}

/** 同步过程中的单行进度。 */
export function formatSyncProgress(progress: SyncProgress): string {
  switch (progress.stage) {
    case 'reconcile':
      return '正在读取服务器清单…';
    case 'push':
      return `正在上传 ${progress.done + 1}/${progress.total}：${shorten(progress.label)}`;
    case 'pull':
      return `正在取 ${progress.done + 1}/${progress.total}：${shorten(progress.label)}`;
    default:
      return '对账完成';
  }
}

export function linkLabel(status: LinkStatus): string {
  switch (status.state) {
    case 'online':
      return '自动同步已连接';
    case 'connecting':
      return '正在连接…';
    case 'offline':
      return status.retry_in > 0 ? `离线，${status.retry_in}s 后重试` : '离线';
    default:
      return '自动同步未启用';
  }
}
