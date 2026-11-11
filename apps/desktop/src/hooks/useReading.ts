import * as React from 'react';
import {
  filterArticles,
  normalizeInputUrl,
  reading,
  unfinishedCount,
  type AnnotationInput,
  type ReadingAnnotation,
  type ReadingArticle,
  type ReadingArticleDetail,
  type ReadingFilter,
} from '../lib/reading';

export interface BatchState {
  running: boolean;
  total: number;
  done: number;
  ok: number;
  failed: number;
  current: string | null;
}

const BATCH_IDLE: BatchState = { running: false, total: 0, done: 0, ok: 0, failed: 0, current: null };

export interface BatchError {
  url: string;
  message: string;
}

/**
 * The library as the app sees it: one list read on demand, the article the
 * reader is showing, and the writes that keep both in step. Progress writes
 * happen on leave / blur, not on every scroll, so the list only ever shows a
 * position the reader actually settled on.
 */
export function useReading() {
  const [articles, setArticles] = React.useState<ReadingArticle[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [filter, setFilter] = React.useState<ReadingFilter>('all');
  const [adding, setAdding] = React.useState(false);
  const [addError, setAddError] = React.useState<string | null>(null);
  const [addNotice, setAddNotice] = React.useState<string | null>(null);
  const [batch, setBatch] = React.useState<BatchState>(BATCH_IDLE);
  const [batchErrors, setBatchErrors] = React.useState<BatchError[]>([]);
  const [busy, setBusy] = React.useState<Set<string>>(() => new Set());
  const [open, setOpen] = React.useState<ReadingArticleDetail | null>(null);
  const [opening, setOpening] = React.useState<string | null>(null);
  const readSequence = React.useRef(0);

  const refresh = React.useCallback(async () => {
    const request = ++readSequence.current;
    try {
      const list = await reading.list();
      if (request !== readSequence.current) return;
      setArticles(list);
      setError(null);
    } catch (caught) {
      if (request !== readSequence.current) return;
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      if (request === readSequence.current) setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
    // 上次退出时还在解析的条目在这里续跑，避免「解析中」永远停住。
    void reading.resumePending().catch(() => {});
  }, [refresh]);

  // 后台解析完成（成功或失败）后刷新列表，让状态徽标与正文一起更新。
  React.useEffect(() => {
    let stop: (() => void) | undefined;
    void reading
      .onParsed(() => {
        void refresh();
      })
      .then((unlisten) => {
        stop = unlisten;
      });
    return () => stop?.();
  }, [refresh]);

  const mark = React.useCallback((id: string, on: boolean) => {
    setBusy((current) => {
      const next = new Set(current);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  }, []);

  const replaceSummary = React.useCallback((summary: ReadingArticle) => {
    setArticles((current) => current.map((article) => (article.id === summary.id ? summary : article)));
    setOpen((current) => (current && current.id === summary.id ? { ...current, ...summary } : current));
  }, []);

  const importBrowserPage = React.useCallback(async (url: string, html: string): Promise<boolean> => {
    setAdding(true);
    setAddError(null);
    setAddNotice(null);
    try {
      const detail = await reading.importBrowserPage(url, html);
      setArticles((current) => {
        const { content_html: _h, markdown: _m, image_dir: _d, annotations: _a, ...summary } = detail;
        return [summary, ...current.filter((article) => article.id !== summary.id)];
      });
      return true;
    } catch (caught) {
      setAddError(caught instanceof Error ? caught.message : String(caught));
      return false;
    } finally {
      setAdding(false);
    }
  }, []);

  const cancelAdd = React.useCallback(() => {
    setAddNotice(null);
    setAddError(null);
  }, []);

  const addBatch = React.useCallback(async (urls: string[]): Promise<void> => {
    if (urls.length === 0) return;
    setBatchErrors([]);
    setBatch({ running: true, total: urls.length, done: 0, ok: 0, failed: 0, current: urls[0] });
    const added: ReadingArticle[] = [];
    const failures: BatchError[] = [];
    for (const url of urls) {
      try {
        added.push(await reading.quickAdd(url));
      } catch (caught) {
        failures.push({ url, message: caught instanceof Error ? caught.message : String(caught) });
      }
    }
    setArticles((current) => {
      const fresh = new Set(added.map((article) => article.id));
      // 后加入的 added_at 更大，排在前面。
      return [...added.slice().reverse(), ...current.filter((article) => !fresh.has(article.id))];
    });
    setBatchErrors(failures);
    setBatch({
      running: false,
      total: urls.length,
      done: urls.length,
      ok: added.length,
      failed: failures.length,
      current: null,
    });
  }, []);

  /** 秒回：先把链接放进列表，正文由 Rust 后台抓取，解析状态随事件更新。 */
  const add = React.useCallback(async (raw: string): Promise<boolean> => {
    const url = normalizeInputUrl(raw);
    if (!url) {
      setAddError('请输入完整的 http/https 链接');
      return false;
    }
    setAdding(true);
    setAddError(null);
    setAddNotice(null);
    try {
      const article = await reading.quickAdd(url);
      setArticles((current) => [article, ...current.filter((item) => item.id !== article.id)]);
      return true;
    } catch (caught) {
      setAddError(caught instanceof Error ? caught.message : String(caught));
      return false;
    } finally {
      setAdding(false);
    }
  }, []);

  /** 解析失败后重试：行立即回到「解析中」，结果由 reading://parsed 事件带回。 */
  const retryParse = React.useCallback(
    async (id: string) => {
      mark(id, true);
      try {
        replaceSummary(await reading.retryParse(id));
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      } finally {
        mark(id, false);
      }
    },
    [mark, replaceSummary],
  );

  const openArticle = React.useCallback(async (id: string) => {
    setOpening(id);
    try {
      const detail = await reading.get(id);
      setOpen(detail);
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setOpening(null);
    }
  }, []);

  const closeArticle = React.useCallback(() => {
    setOpen(null);
    void refresh();
  }, [refresh]);

  const saveProgress = React.useCallback(
    async (id: string, percent: number, anchor: string | null) => {
      try {
        replaceSummary(await reading.setProgress(id, percent, anchor));
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [replaceSummary],
  );

  const setRead = React.useCallback(
    async (id: string, read: boolean) => {
      mark(id, true);
      try {
        replaceSummary(await reading.setRead(id, read));
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      } finally {
        mark(id, false);
      }
    },
    [mark, replaceSummary],
  );

  const remove = React.useCallback(
    async (id: string) => {
      mark(id, true);
      try {
        await reading.remove(id);
        setArticles((current) => current.filter((article) => article.id !== id));
        setOpen((current) => (current && current.id === id ? null : current));
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      } finally {
        mark(id, false);
      }
    },
    [mark],
  );

  const upsertAnnotation = React.useCallback(
    async (input: AnnotationInput): Promise<ReadingAnnotation | null> => {
      try {
        const saved = await reading.upsertAnnotation(input);
        setOpen((current) => {
          if (!current || current.id !== input.article_id) return current;
          const others = current.annotations.filter((item) => item.id !== saved.id);
          const annotations = [...others, saved].sort((a, b) => a.start - b.start || a.created_at - b.created_at);
          return { ...current, annotations, annotation_count: annotations.length };
        });
        return saved;
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
        return null;
      }
    },
    [],
  );

  const deleteAnnotation = React.useCallback(async (articleId: string, id: string) => {
    try {
      await reading.deleteAnnotation(articleId, id);
      setOpen((current) => {
        if (!current || current.id !== articleId) return current;
        const annotations = current.annotations.filter((item) => item.id !== id);
        return { ...current, annotations, annotation_count: annotations.length };
      });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const visible = React.useMemo(() => filterArticles(articles, filter), [articles, filter]);
  const badge = React.useMemo(() => unfinishedCount(articles), [articles]);

  return {
    articles,
    visible,
    badge,
    loading,
    error,
    filter,
    setFilter,
    adding,
    addError,
    addNotice,
    cancelAdd,
    clearAddError: () => {
      setAddError(null);
      setAddNotice(null);
    },
    busy,
    open,
    opening,
    refresh,
    add,
    importBrowserPage,
    addBatch,
    retryParse,
    batch,
    batchErrors,
    openArticle,
    closeArticle,
    /** Replaces the open detail (e.g. after a context save or re-fetch). */
    applyDetail: (next: ReadingArticleDetail) => {
      setOpen((current) => (current && current.id === next.id ? next : current));
    },
    saveProgress,
    setRead,
    remove,
    upsertAnnotation,
    deleteAnnotation,
  };
}

export type ReadingModel = ReturnType<typeof useReading>;
