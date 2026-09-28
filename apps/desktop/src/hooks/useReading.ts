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

  const add = React.useCallback(async (raw: string): Promise<boolean> => {
    const url = normalizeInputUrl(raw);
    if (!url) {
      setAddError('请输入完整的 http/https 链接');
      return false;
    }
    setAdding(true);
    setAddError(null);
    try {
      const detail = await reading.add(url);
      setArticles((current) => {
        const { content_html: _h, markdown: _m, image_dir: _d, annotations: _a, ...summary } = detail;
        const without = current.filter((article) => article.id !== summary.id);
        return [summary, ...without];
      });
      return true;
    } catch (caught) {
      setAddError(caught instanceof Error ? caught.message : String(caught));
      return false;
    } finally {
      setAdding(false);
    }
  }, []);

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
    clearAddError: () => setAddError(null),
    busy,
    open,
    opening,
    refresh,
    add,
    openArticle,
    closeArticle,
    saveProgress,
    setRead,
    remove,
    upsertAnnotation,
    deleteAnnotation,
  };
}

export type ReadingModel = ReturnType<typeof useReading>;
