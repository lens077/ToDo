/**
 * 另存为 HTML / Markdown 的共享逻辑（任务 sv2md9）：目录选择 + reading_export_article。
 * 阅读上下文面板的按钮与阅读正文右键菜单共用；结果通过 onNotice 上报，
 * 由调用方决定在哪里渲染（面板状态行 / 阅读器提示条）。
 */
import * as React from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { isTauriRuntime } from '../lib/readingSync';
import { reading, type ReadingExportFormat } from '../lib/reading';

export interface ExportNotice {
  kind: 'status' | 'error';
  message: string;
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
        onNotice({ kind: 'status', message: `已保存到 ${result.path}（图片 ${result.images} 张）` });
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
