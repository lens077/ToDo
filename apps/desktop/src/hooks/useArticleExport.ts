/**
 * 另存为 HTML / Markdown 的共享逻辑（任务 sv2md9）：目录选择 + reading_export_article。
 * 阅读上下文面板的按钮与阅读正文/列表卡右键菜单共用；结果通过 onNotice 上报，
 * 由调用方决定在哪里渲染（面板状态行 / 阅读器胶囊 / 列表通知条）。
 * 成功时附带文件与包裹目录路径，提示条用它渲染「打开」「打开目录」「复制路径」。
 *
 * 成功后把路径记进 `readingExportRecords`：列表卡片据此常驻「打开目录」。
 * 「打开目录」失败（目录被移动或删除）时上报 `dir-missing`，提示条改为
 * 「重新下载」，走同一个导出流程重存一遍。
 */
import * as React from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { isTauriRuntime } from '../lib/readingSync';
import { reading, resolveExportNaming, type ReadingExportFormat } from '../lib/reading';
import { currentAppSettings } from '../lib/appSettings';
import { recordExport } from '../lib/readingExportRecords';

export interface ExportNotice {
  kind: 'status' | 'error' | 'dir-missing';
  message: string;
  /** 导出成功时的文件路径；提示条据此显示「打开」。 */
  path?: string;
  /** 导出成功时的包裹目录；提示条据此显示「打开目录」「复制路径」。 */
  dir?: string;
  /** `dir-missing` 时用来重存：文章 ID 与上次的格式。 */
  redownload?: { id: string; format: ReadingExportFormat };
}

/** 打开目录失败的统一提示文案：目录被移动或删除时 Rust 侧会拒绝打开。 */
export const DIR_MISSING_MESSAGE = '目录可能被移动或删除，可以重新下载';

/**
 * 另存为的启动器。文章 ID 在每次调用时传入：列表卡右键、正文右键与
 * 「重新下载」走同一条路径，不必为每篇文章各挂一个 hook 实例。
 */
export function useArticleExport(onNotice: (notice: ExportNotice) => void) {
  const [exportBusy, setExportBusy] = React.useState(false);

  const exportArticle = React.useCallback(
    async (articleId: string, format: ReadingExportFormat) => {
      if (!articleId) {
        onNotice({ kind: 'error', message: '没有可保存的文章' });
        return;
      }
      if (!isTauriRuntime()) {
        onNotice({ kind: 'error', message: '浏览器版不支持另存到本机目录，请使用桌面端。' });
        return;
      }
      setExportBusy(true);
      try {
        const dir = await open({ directory: true, multiple: false, title: '选择保存目录（将写入文章的包裹目录：正文 + img/）' });
        if (typeof dir !== 'string') return;
        // 命名在点击时取当前设置：产品后缀（默认）/ 仅标题 / 自定义前后缀。
        const naming = resolveExportNaming(currentAppSettings());
        const result = await reading.exportArticle(articleId, format, dir, naming);
        // 记下这次保存：卡片据此常驻「打开目录」，提示条消失后也能找回。
        recordExport({
          articleId,
          path: result.path,
          dir: result.dir,
          format,
          images: result.images,
          savedAt: Date.now(),
        });
        onNotice({
          kind: 'status',
          message: `已保存到 ${result.path}（图片 ${result.images} 张）`,
          path: result.path,
          dir: result.dir,
          // 「打开目录」失败时提示条要靠它把按钮降级成「重新下载」。
          redownload: { id: articleId, format },
        });
      } catch (cause) {
        onNotice({ kind: 'error', message: cause instanceof Error ? cause.message : String(cause) });
      } finally {
        setExportBusy(false);
      }
    },
    [onNotice],
  );

  return { exportArticle, exportBusy };
}

/** 用系统默认应用打开导出的文件（失败降级为错误提示）。 */
export async function openExportedFile(notice: ExportNotice, onNotice: (notice: ExportNotice) => void): Promise<void> {
  if (!notice.path) return;
  try {
    await reading.openSavedFile(notice.path);
  } catch (cause) {
    onNotice({ kind: 'error', message: cause instanceof Error ? cause.message : String(cause) });
  }
}

/**
 * 在访达 / 资源管理器中显示导出的包裹目录。Rust 侧只放行已存在的目录，
 * 所以打开失败基本等同于「目录不在了」：换成 `dir-missing` 提示，按钮从
 * 「打开目录」变成「重新下载」，而不是只丢一句原始错误。
 */
export async function openExportedDir(
  notice: ExportNotice,
  onNotice: (notice: ExportNotice) => void,
): Promise<void> {
  if (!notice.dir) return;
  try {
    await reading.openSavedDir(notice.dir);
  } catch {
    onNotice({
      kind: 'dir-missing',
      message: DIR_MISSING_MESSAGE,
      dir: notice.dir,
      redownload: notice.redownload,
    });
  }
}
