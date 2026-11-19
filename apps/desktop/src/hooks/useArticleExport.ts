/**
 * 另存为 HTML / Markdown 的共享逻辑（任务 sv2md9）：目录选择 + reading_export_article。
 * 阅读上下文面板的按钮与阅读正文/列表卡右键菜单共用；结果通过 onNotice 上报，
 * 由调用方决定在哪里渲染（面板状态行 / 阅读器胶囊 / 列表通知条）。
 * 成功时附带导出路径，提示条用它渲染「打开」按钮。
 */
import * as React from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { isTauriRuntime } from '../lib/readingSync';
import { reading, type ReadingExportFormat } from '../lib/reading';

export interface ExportNotice {
  kind: 'status' | 'error';
  message: string;
  /** 导出成功时的文件路径；提示条据此显示「打开」。 */
  path?: string;
}

export function useArticleExport(id: string, onNotice: (notice: ExportNotice) => void) {
  const [exportBusy, setExportBusy] = React.useState(false);

  const exportArticle = React.useCallback(
    async (format: ReadingExportFormat) => {
      if (!isTauriRuntime()) {
        onNotice({ kind: 'error', message: '浏览器版不支持另存到本机目录，请使用桌面端。' });
        return;
      }
      setExportBusy(true);
      try {
        const dir = await open({ directory: true, multiple: false, title: '选择保存目录（将写入文章文件与 img/ 图片目录）' });
        if (typeof dir !== 'string') return;
        const result = await reading.exportArticle(id, format, dir);
        onNotice({ kind: 'status', message: `已保存到 ${result.path}（图片 ${result.images} 张）`, path: result.path });
      } catch (cause) {
        onNotice({ kind: 'error', message: cause instanceof Error ? cause.message : String(cause) });
      } finally {
        setExportBusy(false);
      }
    },
    [id, onNotice],
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
