import { extractBatchUrls } from './reading';

/**
 * 打开阅读视图时看一眼剪贴板：里面有 http(s) 链接就问一句要不要导入。
 *
 * 只提示一次：按整段剪贴板文本算指纹，导入或忽略后写入 localStorage。
 * 同一条复制内容再进阅读视图不会再弹，新复制一段才会。
 */
const HANDLED_KEY = 'virgo-clipboard-handled.v1';

export interface ClipboardCandidate {
  urls: string[];
  fingerprint: string;
}

function fingerprintOf(text: string): string {
  const normalized = text.trim();
  let hash = 5381;
  for (let index = 0; index < normalized.length; index += 1) {
    hash = ((hash << 5) + hash + normalized.charCodeAt(index)) | 0;
  }
  return `${normalized.length}:${hash >>> 0}`;
}

export function wasClipboardHandled(fingerprint: string): boolean {
  try {
    return window.localStorage.getItem(HANDLED_KEY) === fingerprint;
  } catch {
    return false;
  }
}

export function markClipboardHandled(fingerprint: string): void {
  try {
    window.localStorage.setItem(HANDLED_KEY, fingerprint);
  } catch {
    // 存储不可用时本次仍可导入，只是下次会再问一次。
  }
}

async function readClipboardText(): Promise<string | null> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke<string | null>('reading_clipboard_text');
    } catch {
      return null;
    }
  }
  // 浏览器预览环境：没有权限就拿不到，静默跳过。
  try {
    return await navigator.clipboard.readText();
  } catch {
    return null;
  }
}

/** 返回值得提示的剪贴板内容；没有链接或已经处理过则返回 null。 */
export async function readClipboardLinks(): Promise<ClipboardCandidate | null> {
  const text = await readClipboardText();
  if (!text || !/https?:\/\//i.test(text)) return null;
  const urls = extractBatchUrls(text, 'all');
  if (urls.length === 0) return null;
  const fingerprint = fingerprintOf(text);
  if (wasClipboardHandled(fingerprint)) return null;
  return { urls, fingerprint };
}
