import * as React from 'react';
import { createTextAnnotator, type TextAnnotation, type TextAnnotator } from '@recogito/text-annotator';
import type { ReadingModel } from '../hooks/useReading';
import { absoluteTime, relativeTime } from '../lib/ai';
import { desktop } from '../lib/desktop';
import {
  anchorFromScroll,
  hostOf,
  numberBlocks,
  parseAnchor,
  percentFromScroll,
  readingState,
  resolveImageSources,
  shouldOfferResume,
  type ReadingAnnotation,
  type ReadingArticleDetail,
} from '../lib/reading';
import { Icon } from './Icon';

interface ReaderViewProps {
  detail: ReadingArticleDetail;
  model: ReadingModel;
}

/** Progress is written this long after the last scroll, and on leave. */
const IDLE_SAVE_MS = 2500;

const HIGHLIGHT_FILL = '#f5a524';

interface Toolbar {
  id: string;
  quote: string;
  top: number;
  left: number;
  pending: boolean;
}

interface PendingSelection {
  quote: string;
  start: number;
  end: number;
  top: number;
  left: number;
}

/**
 * A stored annotation as recogito wants it. Offsets are trusted when they
 * still quote the same text; otherwise the quote is searched for, which is
 * what keeps the mock's quote-only samples and any drifted record usable.
 */
function toRecogito(annotation: ReadingAnnotation, text: string): TextAnnotation | null {
  let { start, end } = annotation;
  if (!(end > start && text.slice(start, end) === annotation.quote)) {
    const found = text.indexOf(annotation.quote);
    if (found < 0) return null;
    start = found;
    end = found + annotation.quote.length;
  }
  return {
    id: annotation.id,
    bodies: annotation.note
      ? [{ id: `${annotation.id}:note`, annotation: annotation.id, purpose: 'commenting', value: annotation.note }]
      : [],
    target: { annotation: annotation.id, selector: [{ quote: annotation.quote, start, end }] },
  };
}

function selectorOf(annotation: TextAnnotation): { quote: string; start: number; end: number } | null {
  const selector = annotation.target.selector[0];
  if (!selector || typeof selector.start !== 'number' || typeof selector.end !== 'number') return null;
  return { quote: selector.quote, start: selector.start, end: selector.end };
}

function rangeOf(annotation: TextAnnotation): Range | null {
  const selector = annotation.target.selector[0] as { range?: Range } | undefined;
  return selector?.range ?? null;
}

interface NoteCardProps {
  annotation: ReadingAnnotation;
  selected: boolean;
  editing: boolean;
  onSelect: () => void;
  onEdit: () => void;
  onCancel: () => void;
  onSave: (note: string) => Promise<void>;
  onDelete: () => void;
}

function NoteCard({ annotation, selected, editing, onSelect, onEdit, onCancel, onSave, onDelete }: NoteCardProps) {
  const [draft, setDraft] = React.useState(annotation.note);
  const [saving, setSaving] = React.useState(false);
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  React.useEffect(() => {
    if (editing) {
      setDraft(annotation.note);
      textareaRef.current?.focus();
    }
  }, [editing, annotation.note]);

  const save = async () => {
    setSaving(true);
    try {
      await onSave(draft.trim());
    } finally {
      setSaving(false);
    }
  };

  return (
    <article
      className={`reader-note${selected ? ' is-selected' : ''}${editing ? ' is-editing' : ''}`}
      data-testid="reader-note"
      data-annotation={annotation.id}
      aria-current={selected ? 'true' : undefined}
    >
      <button type="button" className="reader-note__quote" onClick={onSelect} title="在正文中定位">
        <q>{annotation.quote}</q>
      </button>
      {editing ? (
        <form
          className="reader-note__form"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <textarea
            ref={textareaRef}
            value={draft}
            rows={3}
            placeholder="写下你的想法…"
            aria-label="笔记内容"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Escape') onCancel();
              if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') void save();
            }}
          />
          <div className="reader-note__actions">
            <button type="submit" className="reader-note__save" disabled={saving}>{saving ? '保存中…' : '保存'}</button>
            <button type="button" className="text-action" disabled={saving} onClick={onCancel}>取消</button>
            <button type="button" className="text-action reader-note__delete" disabled={saving} onClick={onDelete}>删除标记</button>
          </div>
        </form>
      ) : (
        <>
          {annotation.note ? (
            <p className="reader-note__text">{annotation.note}</p>
          ) : (
            <button type="button" className="reader-note__add" onClick={onEdit}>添加笔记</button>
          )}
          <footer className="reader-note__meta">
            <time dateTime={new Date(annotation.updated_at).toISOString()} title={absoluteTime(annotation.updated_at)}>
              {relativeTime(annotation.updated_at)}
            </time>
            {annotation.note && (
              <button type="button" className="reader-note__edit" onClick={onEdit}>编辑</button>
            )}
            <button type="button" className="reader-note__edit reader-note__delete" onClick={onDelete}>删除</button>
          </footer>
        </>
      )}
    </article>
  );
}

export function ReaderView({ detail, model }: ReaderViewProps) {
  const paneRef = React.useRef<HTMLDivElement>(null);
  const paperRef = React.useRef<HTMLDivElement>(null);
  const bodyRef = React.useRef<HTMLDivElement>(null);
  const annotatorRef = React.useRef<TextAnnotator<TextAnnotation, TextAnnotation> | null>(null);
  const blocksRef = React.useRef<HTMLElement[]>([]);
  const positionRef = React.useRef<{ percent: number; anchor: string | null; interacted: boolean }>({
    percent: detail.progress_percent,
    anchor: detail.progress_anchor,
    interacted: false,
  });
  const savedRef = React.useRef<{ percent: number; anchor: string | null }>({
    percent: detail.progress_percent,
    anchor: detail.progress_anchor,
  });
  const idleTimer = React.useRef<number | null>(null);
  // A programmatic scroll (restore, 从头开始) is not reading: the one scroll
  // event it raises is swallowed so it neither saves nor dismisses.
  const programmaticAt = React.useRef(0);
  const bannerBaseline = React.useRef(detail.progress_percent);
  const [livePercent, setLivePercent] = React.useState(detail.progress_percent);
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  const [editingId, setEditingId] = React.useState<string | null>(null);
  const [toolbar, setToolbar] = React.useState<Toolbar | null>(null);
  const [pendingSelection, setPendingSelection] = React.useState<PendingSelection | null>(null);
  const [banner, setBanner] = React.useState<'resumed' | null>(null);
  const [copied, setCopied] = React.useState(false);

  const state = readingState(detail);
  const annotations = detail.annotations;
  // The latest annotations for the mount effect, which must not re-run when
  // a note is edited: re-mounting would drop the reader's scroll position.
  const annotationsRef = React.useRef(annotations);
  annotationsRef.current = annotations;
  const modelRef = React.useRef(model);
  modelRef.current = model;

  const flushProgress = React.useCallback(() => {
    const position = positionRef.current;
    if (!position.interacted) return;
    if (position.percent === savedRef.current.percent && position.anchor === savedRef.current.anchor) return;
    savedRef.current = { percent: position.percent, anchor: position.anchor };
    void modelRef.current.saveProgress(detail.id, position.percent, position.anchor);
  }, [detail.id]);

  // ── Mount: body, anchors, annotator, restore ─────────────────────────────
  React.useEffect(() => {
    const body = bodyRef.current;
    const pane = paneRef.current;
    if (!body || !pane) return undefined;
    body.innerHTML = resolveImageSources(detail.content_html, detail.image_dir);
    const blocks = numberBlocks(body);
    blocksRef.current = blocks;

    const annotator = createTextAnnotator<TextAnnotation, TextAnnotation>(body, {
      renderer: 'CSS_HIGHLIGHTS',
      annotatingEnabled: true,
      style: (_annotation, annotationState) => ({
        fill: HIGHLIGHT_FILL,
        fillOpacity: annotationState.selected ? 0.55 : 0.32,
        underlineColor: annotationState.selected ? '#b45309' : undefined,
        underlineThickness: annotationState.selected ? 2 : undefined,
        underlineOffset: 3,
      }),
    });
    annotatorRef.current = annotator;

    const text = body.textContent ?? '';
    const revived = annotationsRef.current
      .map((annotation) => toRecogito(annotation, text))
      .filter((annotation): annotation is TextAnnotation => annotation !== null);
    annotator.setAnnotations(revived);

    const showToolbar = (annotation: TextAnnotation) => {
      const range = rangeOf(annotation);
      const paper = paperRef.current;
      if (!range || !paper) return;
      const rect = range.getBoundingClientRect();
      const paperRect = paper.getBoundingClientRect();
      const selector = selectorOf(annotation);
      setToolbar({
        id: annotation.id,
        quote: selector?.quote ?? '',
        top: rect.top - paperRect.top,
        left: Math.max(8, Math.min(rect.left - paperRect.left + rect.width / 2, paperRect.width - 8)),
        pending: false,
      });
    };

    const onCreate = (annotation: TextAnnotation) => {
      const selector = selectorOf(annotation);
      const range = rangeOf(annotation);
      const paper = paperRef.current;
      if (!selector || !selector.quote.trim() || !range || !paper) {
        annotator.removeAnnotation(annotation.id);
        return;
      }
      const rect = range.getBoundingClientRect();
      const paperRect = paper.getBoundingClientRect();
      // Recogito creates a temporary annotation so the selected range remains
      // available while the user chooses 标记 / 标记并写笔记.
      annotator.removeAnnotation(annotation.id);
      setPendingSelection({
        ...selector,
        top: rect.top - paperRect.top,
        left: Math.max(8, Math.min(rect.left - paperRect.left + rect.width / 2, paperRect.width - 8)),
      });
    };
    const onUpdate = (annotation: TextAnnotation) => {
      const selector = selectorOf(annotation);
      if (!selector) return;
      const known = annotationsRef.current.find((item) => item.id === annotation.id);
      const note = known?.note ?? '';
      // Programmatic body syncs from the notes pane echo back here; only a
      // changed target is worth a write.
      if (known && known.quote === selector.quote && known.start === selector.start && known.end === selector.end) return;
      void modelRef.current.upsertAnnotation({ id: annotation.id, article_id: detail.id, ...selector, note });
    };
    const onDelete = (annotation: TextAnnotation) => {
      void modelRef.current.deleteAnnotation(detail.id, annotation.id);
      setToolbar((current) => (current?.id === annotation.id ? null : current));
    };
    const onSelectionChanged = (selected: TextAnnotation[]) => {
      const first = selected[0];
      setSelectedId(first ? first.id : null);
      if (!first) setToolbar(null);
    };
    const onClick = (annotation: TextAnnotation) => {
      showToolbar(annotation);
    };
    annotator.on('createAnnotation', onCreate);
    annotator.on('updateAnnotation', onUpdate);
    annotator.on('deleteAnnotation', onDelete);
    annotator.on('selectionChanged', onSelectionChanged);
    annotator.on('clickAnnotation', onClick);

    // Restore: anchor first, percentage as the fallback, top when unread.
    const anchorIndex = parseAnchor(detail.progress_anchor);
    const scrollTo = (top: number) => {
      const target = Math.max(0, Math.round(top));
      if (Math.abs(pane.scrollTop - target) < 2) return;
      programmaticAt.current = performance.now();
      pane.scrollTop = target;
    };
    const restore = () => {
      if (detail.read_at !== null || detail.progress_percent >= 100) {
        scrollTo(0);
        return;
      }
      if (anchorIndex !== null && blocks[anchorIndex]) {
        const block = blocks[anchorIndex];
        block.classList.add('reader-last-read');
        const offset = block.getBoundingClientRect().top - pane.getBoundingClientRect().top + pane.scrollTop;
        scrollTo(offset - 12);
      } else if (detail.progress_percent > 0) {
        scrollTo(((detail.progress_percent / 100) * pane.scrollHeight) - pane.clientHeight);
      }
      bannerBaseline.current = percentFromScroll(pane);
      if (shouldOfferResume(detail)) setBanner('resumed');
    };
    restore();
    // Images settle the layout after first paint; re-anchor once they do.
    const settle = window.setTimeout(restore, 400);

    return () => {
      window.clearTimeout(settle);
      annotator.off('createAnnotation', onCreate);
      annotator.off('updateAnnotation', onUpdate);
      annotator.off('deleteAnnotation', onDelete);
      annotator.off('selectionChanged', onSelectionChanged);
      annotator.off('clickAnnotation', onClick);
      annotator.destroy();
      annotatorRef.current = null;
      body.innerHTML = '';
    };
    // Mount once per article; annotation edits flow through the annotator API.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.id]);

  // Notes edited in the side pane are pushed into the annotator's bodies so
  // its state (and any future export) stays whole.
  React.useEffect(() => {
    const annotator = annotatorRef.current;
    if (!annotator) return;
    for (const annotation of annotations) {
      const existing = annotator.getAnnotationById(annotation.id);
      if (!existing) continue;
      const value = existing.bodies[0]?.value ?? '';
      if (value !== annotation.note) {
        annotator.updateAnnotation({
          ...existing,
          bodies: annotation.note
            ? [{ id: `${annotation.id}:note`, annotation: annotation.id, purpose: 'commenting', value: annotation.note }]
            : [],
        });
      }
    }
  }, [annotations]);

  // ── Progress tracking ───────────────────────────────────────────────────
  React.useEffect(() => {
    const pane = paneRef.current;
    if (!pane) return undefined;
    let frame: number | null = null;
    const measure = () => {
      frame = null;
      if (programmaticAt.current && performance.now() - programmaticAt.current < 200) {
        programmaticAt.current = 0;
        return;
      }
      const percent = percentFromScroll(pane);
      const anchor = anchorFromScroll(blocksRef.current, pane);
      positionRef.current = { percent, anchor, interacted: true };
      setLivePercent(percent);
      setBanner((current) => (current && Math.abs(percent - bannerBaseline.current) > 15 ? null : current));
      if (idleTimer.current !== null) window.clearTimeout(idleTimer.current);
      idleTimer.current = window.setTimeout(flushProgress, IDLE_SAVE_MS);
    };
    const onScroll = () => {
      if (frame === null) frame = window.requestAnimationFrame(measure);
    };
    const onHide = () => {
      if (document.visibilityState === 'hidden') flushProgress();
    };
    pane.addEventListener('scroll', onScroll, { passive: true });
    document.addEventListener('visibilitychange', onHide);
    window.addEventListener('blur', flushProgress);
    return () => {
      pane.removeEventListener('scroll', onScroll);
      document.removeEventListener('visibilitychange', onHide);
      window.removeEventListener('blur', flushProgress);
      if (frame !== null) window.cancelAnimationFrame(frame);
      if (idleTimer.current !== null) window.clearTimeout(idleTimer.current);
      flushProgress();
    };
  }, [detail.id, flushProgress]);

  // ── Actions ─────────────────────────────────────────────────────────────
  const focusAnnotation = (id: string) => {
    const annotator = annotatorRef.current;
    if (!annotator) return;
    annotator.setSelected(id);
    annotator.scrollIntoView(id, paneRef.current ?? undefined);
    setSelectedId(id);
    setToolbar(null);
  };

  const removeAnnotation = (id: string) => {
    // The annotator's deleteAnnotation event carries the write to the store.
    annotatorRef.current?.removeAnnotation(id);
    setToolbar(null);
    if (editingId === id) setEditingId(null);
  };

  const commitSelection = (withNote: boolean) => {
    if (!pendingSelection) return;
    const id = `annotation-${Date.now().toString(36)}`;
    const { quote, start, end } = pendingSelection;
    void model.upsertAnnotation({ id, article_id: detail.id, quote, start, end, note: '' });
    annotatorRef.current?.addAnnotation({
      id,
      bodies: [],
      target: { annotation: id, selector: [{ quote, start, end }] },
    });
    setSelectedId(id);
    setToolbar({ ...pendingSelection, id, pending: false });
    setPendingSelection(null);
    if (withNote) setEditingId(id);
  };

  const saveNote = async (annotation: ReadingAnnotation, note: string) => {
    await model.upsertAnnotation({
      id: annotation.id,
      article_id: detail.id,
      quote: annotation.quote,
      start: annotation.start,
      end: annotation.end,
      note,
    });
    setEditingId(null);
  };

  const beginNote = (id: string) => {
    setToolbar(null);
    setEditingId(id);
    setSelectedId(id);
    window.setTimeout(() => {
      document.querySelector<HTMLElement>(`[data-annotation="${id}"]`)?.scrollIntoView({ block: 'nearest' });
    }, 0);
  };

  const handleArticleLink = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = (event.target as HTMLElement).closest<HTMLAnchorElement>('a[href]');
    if (!target || !bodyRef.current?.contains(target)) return;
    const href = target.href;
    if (!/^https?:\/\//i.test(href)) {
      event.preventDefault();
      return;
    }
    const isInternal = new URL(href).hostname === 'mp.weixin.qq.com';
    if (isInternal) {
      event.preventDefault();
      const confirmed = window.confirm('这是另一篇微信文章。点击“确定”将在浏览器中打开；需要离线阅读请复制链接回到阅读列表添加。');
      if (confirmed) void desktop.openExternal(href);
    }
  };

  const copyQuote = async (quote: string) => {
    try {
      await navigator.clipboard.writeText(quote);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard access is best effort.
    }
  };

  const toStart = () => {
    const pane = paneRef.current;
    if (pane && pane.scrollTop > 0) {
      programmaticAt.current = performance.now();
      pane.scrollTop = 0;
    }
    positionRef.current = { percent: 0, anchor: null, interacted: true };
    setLivePercent(0);
    setBanner(null);
  };

  const leave = () => {
    flushProgress();
    model.closeArticle();
  };

  const busy = model.busy.has(detail.id);
  const shownPercent = state === 'read' ? 100 : livePercent;

  return (
    <section className="reader" aria-labelledby="reader-title">
      <header className="reader__bar">
        <button type="button" className="reader__back text-action" onClick={leave}>
          <Icon name="arrow" className="reader__back-icon" /> 返回列表
        </button>
        <div className="reader__heading">
          <h1 id="reader-title" title={detail.title}>{detail.title}</h1>
          <p className="reader__meta">
            <span title={detail.url}>{detail.site_name || hostOf(detail.url)}</span>
            {detail.byline && detail.byline !== detail.site_name && (
              <>
                <span aria-hidden="true">·</span>
                <span>{detail.byline}</span>
              </>
            )}
            <span aria-hidden="true">·</span>
            <span>{detail.reading_minutes} 分钟</span>
            <span aria-hidden="true">·</span>
            <span>{state === 'read' ? '已读' : `已读 ${shownPercent}%`}</span>
          </p>
        </div>
        <div className="reader__actions">
          <button type="button" className="text-action" onClick={() => void desktop.openExternal(detail.url)} title={detail.url}>
            <Icon name="external" /> 在浏览器打开
          </button>
          {state === 'read' ? (
            <button type="button" className="text-action" disabled={busy} onClick={() => void model.setRead(detail.id, false)}>
              <Icon name="refresh" /> 标为未读
            </button>
          ) : (
            <button type="button" className="reader__done" disabled={busy} onClick={() => void model.setRead(detail.id, true)}>
              <Icon name="check" /> 标为已读
            </button>
          )}
        </div>
        <div className="reader__progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={shownPercent} aria-label="阅读进度">
          <span style={{ width: `${shownPercent}%` }} />
        </div>
      </header>

      <div className="reader__columns">
        <div className="reader-pane" ref={paneRef} tabIndex={-1}>
          {banner === 'resumed' && (
            <div className="reader-resume" role="status">
              <span>已回到上次读到的位置 · {detail.progress_percent}%</span>
              <button type="button" onClick={toStart}>从头开始</button>
              <button type="button" className="reader-resume__close" aria-label="关闭提示" onClick={() => setBanner(null)}>
                <Icon name="close" />
              </button>
            </div>
          )}
          <div className="reader-paper" ref={paperRef}>
            <div className="reader-body" ref={bodyRef} data-testid="reader-body" onClick={handleArticleLink} />
            {pendingSelection && (
              <div className="reader-toolbar reader-toolbar--selection" role="toolbar" aria-label="标记操作" style={{ top: pendingSelection.top, left: pendingSelection.left }}>
                <button type="button" onClick={() => commitSelection(false)}><Icon name="highlighter" /> 标记</button>
                <button type="button" onClick={() => commitSelection(true)}><Icon name="capture" /> 标记并写笔记</button>
                <button type="button" onClick={() => void copyQuote(pendingSelection.quote)}>{copied ? '已复制' : '复制'}</button>
              </div>
            )}
            {toolbar && !pendingSelection && (
              <div className="reader-toolbar" role="toolbar" aria-label="标记操作" style={{ top: toolbar.top, left: toolbar.left }}>
                <button type="button" onClick={() => beginNote(toolbar.id)}><Icon name="capture" /> 写笔记</button>
                <button type="button" onClick={() => void copyQuote(toolbar.quote)}>{copied ? '已复制' : '复制'}</button>
                <button type="button" className="reader-toolbar__remove" onClick={() => removeAnnotation(toolbar.id)}><Icon name="trash" /> 取消标记</button>
              </div>
            )}
          </div>
        </div>

        <aside className="reader-notes" aria-label="标记与笔记">
          <header className="reader-notes__header">
            <h2>标记与笔记 <span>{annotations.length}</span></h2>
          </header>
          {annotations.length === 0 ? (
            <p className="reader-notes__empty">选中正文任意段落即可添加标记，再为标记写笔记。</p>
          ) : (
            <div className="reader-notes__list">
              {annotations.map((annotation) => (
                <NoteCard
                  key={annotation.id}
                  annotation={annotation}
                  selected={selectedId === annotation.id}
                  editing={editingId === annotation.id}
                  onSelect={() => focusAnnotation(annotation.id)}
                  onEdit={() => beginNote(annotation.id)}
                  onCancel={() => setEditingId(null)}
                  onSave={(note) => saveNote(annotation, note)}
                  onDelete={() => removeAnnotation(annotation.id)}
                />
              ))}
            </div>
          )}
          <p className="reader-notes__hint"><Icon name="highlighter" /> 选中正文任意段落即可添加标记</p>
        </aside>
      </div>
    </section>
  );
}
