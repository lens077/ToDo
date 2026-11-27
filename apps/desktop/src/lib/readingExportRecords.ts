/**
 * 阅读文章「已保存」记录：记住每篇文章最近一次「另存为」落在哪个包裹目录，
 * 让列表卡片在保存后常驻「打开目录」，而不是只能靠一次性的成功提示条
 * （提示条 6~12 秒后会消失，用户往往已经找不到刚存的目录）。
 *
 * 这里只记录本地路径与时间，不触碰阅读库本体：目录可能被用户在访达 /
 * 资源管理器里移动或删除，那由「打开目录」失败时的提示 + 重新下载处理。
 * 数据存 localStorage，与项目文件无关。
 */
import type { ReadingExportFormat } from './reading';

const STORAGE_KEY = 'virgo.reading.exports';

export interface ReadingExportRecord {
  /** 文章 ID（阅读库内部 ID）。 */
  articleId: string;
  /** 主文件绝对路径。 */
  path: string;
  /** 包裹目录绝对路径：正文文件与 `img/` 都在这里面。 */
  dir: string;
  /** 上次导出的格式，「重新下载」按原格式重存。 */
  format: ReadingExportFormat;
  /** 保存进 `img/` 的图片数，卡片上用作「图片 N 张」的依据。 */
  images: number;
  /** 最后一次保存成功的 epoch 毫秒。 */
  savedAt: number;
}

type Records = Record<string, ReadingExportRecord>;

function readAll(): Records {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    return parsed as Records;
  } catch {
    // 存储被禁用或内容损坏：当作没有记录，功能降级为「只认本次提示条」。
    return {};
  }
}

function writeAll(records: Records): boolean {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(records));
    return true;
  } catch {
    return false;
  }
}

/** 记下一次成功的另存为；同一篇文章后来的保存覆盖前面的记录。 */
export function recordExport(record: ReadingExportRecord): void {
  const records = readAll();
  records[record.articleId] = record;
  writeAll(records);
}

/** 某篇文章最近一次的另存为记录；没有则返回 null。 */
export function getExportRecord(articleId: string): ReadingExportRecord | null {
  return readAll()[articleId] ?? null;
}

/** 批量取记录：列表一次渲染只读一遍 localStorage。 */
export function listExportRecords(): Records {
  return readAll();
}

/** 文章被移出阅读库时清掉它的记录，避免同一 ID 再次出现时指向旧目录。 */
export function clearExportRecord(articleId: string): void {
  const records = readAll();
  if (!(articleId in records)) return;
  delete records[articleId];
  writeAll(records);
}
