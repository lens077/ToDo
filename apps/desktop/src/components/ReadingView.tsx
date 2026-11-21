import * as React from 'react';
import type { InboxKindFilter, ReadingModel } from '../hooks/useReading';
import { absoluteTime, relativeTime } from '../lib/ai';
import {
  BATCH_SCOPE_LABEL,
  extractBatchUrls,
  normalizeInputUrl,
  READING_FILTER_LABEL,
  countByState,
  coverSource,
  hostOf,
  readingState,
  type BatchScope,
  type ReadingArticle,
  type ReadingFilter,
} from '../lib/reading';
import { Icon } from './Icon';
import { ReaderContextMenu } from './ReaderContextMenu';
import { useArticleExport, openExportedFile, openExportedDir, type ExportNotice } from '../hooks/useArticleExport';
import { useAppSettings } from '../lib/appSettings';
import {
  markClipboardHandled,
  readClipboardLinks,
  type ClipboardCandidate,
} from '../lib/clipboardLinks';
import {
  cancelSync,
  clearSyncConfig,
  formatSyncProgress,
  formatSyncReport,
  isTauriRuntime,
  linkLabel,
  onSyncEvents,
  readSyncConfig,
  startSyncWatcher,
  stopSyncWatcher,
  writeSyncConfig,
  type LinkStatus,
  type SyncProgress,
  type SyncReport,
} from '../lib/readingSync';

interface ReadingViewProps {
  model: ReadingModel;
  /** 随记视图（分拣面板）入口：随记视图已移出导航，从这里进（设计稿 §6）。 */
  onOpenTriage: () => void;
}

const FILTER_ORDER: ReadingFilter[] = ['all', 'unread', 'reading', 'read'];
const KIND_ORDER: InboxKindFilter[] = ['all', 'article', 'note'];
const KIND_LABEL: Record<InboxKindFilter, string> = { all: '全部', article: '文章', note: '随记' };

function Cover({ article }: { article: ReadingArticle }) {
  const [failed, setFailed] = React.useState(false);
  const source = coverSource(article);
  if (!source || failed) {
    return (
      <span className="reading-card__cover reading-card__cover--empty" aria-hidden="true">
        <Icon name="book" />
      </span>
    );
  }
  return <img className="reading-card__cover" src={source} alt="" loading="lazy" onError={() => setFailed(true)} />;
}

interface ReadingCardProps {
  article: ReadingArticle;
  busy: boolean;
  opening: boolean;
  onOpen: (id: string) => void;
  onSetRead: (id: string, read: boolean) => void;
  onRemove: (id: string) => void;
  onRetry: (id: string) => void;
  /** 右键卡片直接弹「另存为」菜单，不必先进文章（sv2md9）。 */
  onExportMenu: (id: string, position: { x: number; y: number }) => void;
}

/** 微信的 og:description 是「原创 作者 公众号」连排，主次作者之间补间隔符。 */
function displayExcerpt(excerpt: string): string {
  return excerpt.replace(/^(原创[，,]?\s*)(\S+)\s+(.+)$/, '$1$2 · $3');
}

function ReadingCard({ article, busy, opening, onOpen, onSetRead, onRemove, onRetry, onExportMenu }: ReadingCardProps) {
  const state = readingState(article);
  const percent = state === 'read' ? 100 : article.progress_percent;
  const parsing = article.parse_status === 'pending';
  const failed = article.parse_status === 'failed';
  const unavailable = parsing || failed;
  const action =
    state === 'read' ? '重读' : state === 'reading' ? `继续阅读 · ${article.progress_percent}%` : '开始阅读';
  return (
    <article
      className={`reading-card reading-card--${state}${busy ? ' is-busy' : ''}${unavailable ? ' is-unparsed' : ''}`}
      data-testid="reading-card"
      data-article={article.id}
      data-parse-status={article.parse_status}
      aria-label={article.title}
      onContextMenu={(event) => {
        event.preventDefault();
        onExportMenu(article.id, { x: event.clientX, y: event.clientY });
      }}
    >
      <button type="button" className="reading-card__open" onClick={() => onOpen(article.id)} disabled={busy || opening || unavailable} aria-label={`打开：${article.title}`}>
        <Cover article={article} />
        <span className="reading-card__body">
          <span className="reading-card__title">{article.title}</span>
          {article.excerpt && <span className="reading-card__excerpt">{displayExcerpt(article.excerpt)}</span>}
          {failed && article.parse_error && (
            <span className="reading-card__parse-reason" title={article.parse_error}>{article.parse_error}</span>
          )}
          <span className="reading-card__meta">
            {parsing && <span className="reading-card__parse reading-card__parse--pending">解析中…</span>}
            {failed && <span className="reading-card__parse reading-card__parse--failed">解析失败</span>}
            <span title={article.url}>{article.site_name || hostOf(article.url)}</span>
            {article.word_count > 0 && (
              <>
                <span aria-hidden="true">·</span>
                <span>{article.reading_minutes} 分钟</span>
              </>
            )}
            <span aria-hidden="true">·</span>
            <time dateTime={new Date(article.added_at).toISOString()} title={absoluteTime(article.added_at)}>
              {relativeTime(article.added_at)}
            </time>
            {article.annotation_count > 0 && (
              <>
                <span aria-hidden="true">·</span>
                {/* annotation_count 是标记总数，不是写了笔记的条数；按标记数说，
                    和阅读页「标记与笔记」页头一致。 */}
                <span className="reading-card__notes"><Icon name="capture" />{article.annotation_count} 处标记</span>
              </>
            )}
          </span>
        </span>
      </button>
      <div className="reading-card__side">
        {unavailable ? (
          failed ? (
            <button type="button" className="text-action text-action--primary" disabled={busy} onClick={() => onRetry(article.id)}>
              重试解析
            </button>
          ) : (
            <span className="reading-chip reading-chip--pending">解析中…</span>
          )
        ) : state === 'read' ? (
          <span className="reading-chip reading-chip--read"><Icon name="check" />已读</span>
        ) : (
          <button type="button" className={`text-action${state === 'reading' ? ' text-action--primary' : ''}`} disabled={busy || opening} onClick={() => onOpen(article.id)}>
            {opening ? '打开中…' : action}
          </button>
        )}
        <div className="reading-card__tools">
          <button
            type="button"
            className="icon-action"
            disabled={busy || parsing}
            data-tip={parsing ? '解析中…' : '重新解析：重新抓取正文与图片，笔记与上下文保留'}
            aria-label={`重新解析：${article.title}`}
            onClick={() => onRetry(article.id)}
          >
            <Icon name="refresh" />
          </button>
          <button
            type="button"
            className="icon-action"
            disabled={busy}
            data-tip={state === 'read' ? '标为未读：回到未读列表' : '标为已读：直接完成，不再出现在待读里'}
            aria-label={state === 'read' ? `标为未读：${article.title}` : `标为已读：${article.title}`}
            onClick={() => onSetRead(article.id, state !== 'read')}
          >
            <Icon name={state === 'read' ? 'refresh' : 'check'} />
          </button>
          <button
            type="button"
            className="icon-action"
            disabled={busy}
            data-tip="删除：移出阅读库，本地正文、图片与笔记一并删除"
            aria-label={`删除：${article.title}`}
            onClick={() => {
              if (window.confirm(`删除「${article.title}」？本地正文、图片与笔记都会一并删除。`)) onRemove(article.id);
            }}
          >
            <Icon name="trash" />
          </button>
        </div>
      </div>
      <span className="reading-card__progress" aria-hidden="true">
        <span style={{ width: `${percent}%` }} />
      </span>
    </article>
  );
}

/** 随记卡（统一收件箱 A′）：没有封面与进度，点击进编辑抽屉。 */
function NoteCard({
  article,
  busy,
  onEdit,
  onRemove,
}: {
  article: ReadingArticle;
  busy: boolean;
  onEdit: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  return (
    <article
      className="reading-card reading-card--note"
      data-testid="reading-card"
      data-article={article.id}
      aria-label={`随记：${article.title}`}
    >
      <button type="button" className="reading-card__open" onClick={() => onEdit(article.id)} disabled={busy} aria-label={`编辑随记：${article.title}`}>
        <span className="reading-card__note-pad" aria-hidden="true">MD</span>
        <span className="reading-card__body">
          <span className="reading-card__title">
            <span className="inbox-kind inbox-kind--note">随记</span> {article.title}
          </span>
          {article.excerpt && <span className="reading-card__excerpt">{article.excerpt}</span>}
          <span className="reading-card__meta">
            <time dateTime={new Date(article.updated_at ?? article.added_at).toISOString()} title={absoluteTime(article.updated_at ?? article.added_at)}>
              {relativeTime(article.updated_at ?? article.added_at)}
            </time>
            {article.updated_at !== null && article.updated_at !== article.added_at && (
              <>
                <span aria-hidden="true">·</span>
                <span className="inbox-kind inbox-kind--note">已编辑</span>
              </>
            )}
          </span>
        </span>
      </button>
      <div className="reading-card__side">
        <div className="reading-card__tools">
          <button
            type="button"
            className="icon-action"
            disabled={busy}
            data-tip="编辑随记：Markdown 源码直编，保存后全端可见"
            aria-label={`编辑随记：${article.title}`}
            onClick={() => onEdit(article.id)}
          >
            <Icon name="capture" />
          </button>
          <button
            type="button"
            className="icon-action"
            disabled={busy}
            data-tip="删除：移出收件箱"
            aria-label={`删除随记：${article.title}`}
            onClick={() => {
              if (window.confirm(`删除随记「${article.title}」？`)) onRemove(article.id);
            }}
          >
            <Icon name="trash" />
          </button>
        </div>
      </div>
    </article>
  );
}

/** 随记编辑抽屉（zj3fjt 贴底抽屉模式）：Markdown 源码直编，工具条在光标处插语法。 */
function NoteDrawer({
  draft,
  saving,
  lastEdited,
  onChange,
  onSave,
  onDelete,
  onClose,
}: {
  draft: string;
  saving: boolean;
  lastEdited: number | null;
  onChange: (markdown: string) => void;
  onSave: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const dialogRef = React.useRef<HTMLDialogElement>(null);
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  React.useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || dialog.open) return;
    dialog.showModal();
    // showModal 会把初始焦点给第一个可聚焦元素（关闭按钮），这里显式交给
    // 正文输入区：打开即可输入，关闭按钮不出现默认焦点描边。
    textareaRef.current?.focus();
  }, []);
  // 工具条：在光标处插语法片段；有选区时包裹选中文本。
  const insert = (before: string, after: string = '') => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const selected = draft.slice(start, end);
    const next = draft.slice(0, start) + before + selected + after + draft.slice(end);
    onChange(next);
    requestAnimationFrame(() => {
      textarea.focus();
      const caret = start + before.length + selected.length;
      textarea.setSelectionRange(caret, caret);
    });
  };
  return (
    <dialog ref={dialogRef} className="inbox-drawer" aria-label="编辑随记" onClose={onClose}>
      <div className="inbox-drawer__head">
        <span className="inbox-kind inbox-kind--note">随记</span>
        <span className="inbox-drawer__title">编辑随记</span>
        {lastEdited !== null && (
          <time className="inbox-drawer__edited" dateTime={new Date(lastEdited).toISOString()} title={absoluteTime(lastEdited)}>
            最后编辑 {relativeTime(lastEdited)}
          </time>
        )}
        <button type="button" className="icon-action" aria-label="关闭" onClick={onClose}>
          <Icon name="close" />
        </button>
      </div>
      <textarea
        ref={textareaRef}
        className="inbox-drawer__body"
        value={draft}
        spellCheck={false}
        aria-label="随记正文（Markdown）"
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
            event.preventDefault();
            onSave();
          }
        }}
      />
      <div className="inbox-drawer__tools">
        <button type="button" onClick={() => insert('**', '**')}><strong>B</strong></button>
        <button type="button" onClick={() => insert('*', '*')}><em>I</em></button>
        <button type="button" onClick={() => insert('\n- ', '')}>• 列表</button>
        <button type="button" onClick={() => insert('\n- [ ] ', '')}>☑ 任务</button>
        <button type="button" onClick={() => insert('\n---\n', '')}>— 线</button>
      </div>
      <div className="inbox-drawer__foot">
        <button type="button" className="text-action" style={{ color: 'var(--danger)' }} disabled={saving} onClick={onDelete}>
          删除
        </button>
        <span className="inbox-drawer__spacer" />
        <span className="inbox-hint"><kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>Enter</kbd> 保存</span>
        <button type="button" className="solid-action" disabled={saving || !draft.trim()} onClick={onSave}>
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
    </dialog>
  );
}

export function ReadingView({ model, onOpenTriage }: ReadingViewProps) {
  const [draft, setDraft] = React.useState('');
  const inputRef = React.useRef<HTMLInputElement>(null);
  const [batchOpen, setBatchOpen] = React.useState(false);
  const [batchScope, setBatchScope] = React.useState<BatchScope>('all');
  const [batchDraft, setBatchDraft] = React.useState('');
  const [clipboard, setClipboard] = React.useState<ClipboardCandidate | null>(null);
  const [settings] = useAppSettings();
  // 随记编辑抽屉：id + 当前 Markdown 草稿；「转为任务」走随记视图的分拣面板（§6）。
  const [noteEdit, setNoteEdit] = React.useState<{ id: string; title: string; markdown: string; lastEdited: number | null } | null>(null);
  const [noteSaving, setNoteSaving] = React.useState(false);
  // 列表卡片右键另存为（sv2md9）：菜单记录文章与弹出位置，导出结果与阅读页同一套提示。
  const [exportMenu, setExportMenu] = React.useState<{ id: string; x: number; y: number } | null>(null);
  const [exportNotice, setExportNotice] = React.useState<ExportNotice | null>(null);
  const reportExportNotice = React.useCallback((notice: ExportNotice) => setExportNotice(notice), []);
  const { exportArticle, exportBusy } = useArticleExport(exportMenu?.id ?? '', reportExportNotice);
  // 带「打开」按钮的成功通知停留更久，其余 6 秒。
  React.useEffect(() => {
    if (!exportNotice) return undefined;
    const ttl = exportNotice.path ? 12000 : 6000;
    const timer = window.setTimeout(() => setExportNotice(null), ttl);
    return () => window.clearTimeout(timer);
  }, [exportNotice]);
  // 窄屏 composer 的 placeholder 换短文案（长文案在 320px 宽度下会被截断）。
  const [narrow] = React.useState(() => window.matchMedia('(max-width: 719px)').matches);
  // 「从浏览器保存」是桌面交互（连弹两个 prompt 拿 URL 与 outerHTML）；
  // Android/iOS WebView 上会弹原生 prompt（验收截图 2026-11-21），移动端隐藏。
  const isMobilePlatform = /android|iphone|ipad/i.test(navigator.userAgent);
  const counts = React.useMemo(() => countByState(model.articles), [model.articles]);
  const kindCounts = React.useMemo(
    () => ({
      article: model.articles.filter((article) => article.kind === 'article').length,
      note: model.articles.filter((article) => article.kind === 'note').length,
    }),
    [model.articles],
  );
  const batchUrls = React.useMemo(() => extractBatchUrls(batchDraft, batchScope), [batchDraft, batchScope]);
  // A′ 识别：单一 http/https 前缀 → 文章；其余文本 → 随记。多条链接走批量
  // 管线（按钮文案随路由走，避免「贴两条链接却显示存入随记」的错位）。
  const detectedUrl = React.useMemo(() => normalizeInputUrl(draft), [draft]);
  const isLinkDraft =
    draft.trim() !== '' && (detectedUrl !== null || extractBatchUrls(draft, 'all').length > 1);

  // 每次进阅读视图看一眼剪贴板；同一段内容只在第一次提示（指纹写在 localStorage）。
  // 设置里关掉剪贴板读取后不再调用任何读取接口。
  React.useEffect(() => {
    if (!settings.clipboardImport) {
      setClipboard(null);
      return;
    }
    let cancelled = false;
    void readClipboardLinks().then((candidate) => {
      if (!cancelled) setClipboard(candidate);
    });
    return () => {
      cancelled = true;
    };
  }, [settings.clipboardImport]);

  const dismissClipboard = () => {
    if (clipboard) markClipboardHandled(clipboard.fingerprint);
    setClipboard(null);
  };

  const importClipboard = async () => {
    if (!clipboard) return;
    markClipboardHandled(clipboard.fingerprint);
    const urls = clipboard.urls;
    setClipboard(null);
    if (urls.length === 1) await model.add(urls[0]);
    else await model.addBatch(urls);
  };

  const submit = async () => {
    if (model.adding || model.batch.running || !draft.trim()) return;
    // 一次粘进多条（多行会被输入框折成空格）：逐条入队，而不是拼成一条坏链接。
    const pasted = extractBatchUrls(draft, 'all');
    if (pasted.length > 1) {
      setDraft('');
      inputRef.current?.focus();
      await model.addBatch(pasted);
      return;
    }
    // A′ 分流：识别出链接 → 文章（后台解析）；其余文本 → 随记（Markdown 直存）。
    const saved = detectedUrl
      ? await model.add(detectedUrl)
      : await model.noteAppend(draft);
    if (saved) {
      setDraft('');
      inputRef.current?.focus();
    }
  };

  const countFor = (filter: ReadingFilter) => (filter === 'all' ? model.articles.length : counts[filter]);
  const countForKind = (kind: InboxKindFilter) =>
    kind === 'all' ? model.articles.length : kindCounts[kind];

  /** A′ 工具行「打开文件」：dialog 选本地文件 → 导入管线（html/md 文章、txt 随记）。 */
  const openLocalFile = async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const path = await open({
      multiple: false,
      title: '导入到收件箱',
      filters: [{ name: '文本内容', extensions: ['html', 'htm', 'md', 'markdown', 'txt'] }],
    });
    if (!path || Array.isArray(path)) return;
    await model.importFile(path);
  };

  /** 随记编辑抽屉：拉取 Markdown 草稿（随记没有阅读页，不走 openArticle）。 */
  const editNote = async (id: string) => {
    try {
      const detail = await model.getDetail(id);
      setNoteEdit({ id: detail.id, title: detail.title, markdown: detail.markdown ?? '', lastEdited: detail.updated_at });
    } catch (caught) {
      void caught; // 取稿失败保持抽屉关闭，错误由下一次操作提示
    }
  };

  const saveNote = async () => {
    if (!noteEdit || noteSaving) return;
    setNoteSaving(true);
    const ok = await model.noteUpdate(noteEdit.id, noteEdit.markdown);
    setNoteSaving(false);
    if (ok) setNoteEdit(null);
  };

  const removeNote = async () => {
    if (!noteEdit) return;
    if (!window.confirm(`删除随记「${noteEdit.title}」？`)) return;
    await model.remove(noteEdit.id);
    setNoteEdit(null);
  };

  const importCurrentBrowserPage = async () => {
    const url = window.prompt('粘贴浏览器当前页面的 URL');
    if (!url) return;
    const html = window.prompt('粘贴浏览器复制的页面 HTML（开发者工具中复制 outerHTML）');
    if (!html) return;
    await model.importBrowserPage(url, html);
  };

  return (
    <section className="reading-view" aria-labelledby="reading-title">
      {/* A′ 定稿头部：单行制（标题 + 统计 + 同步状态芯片），说明降级为输入条 hint。 */}
      <header className="inbox-head" id="reading-title">
        <h1>收件箱</h1>
        <span className="inbox-stats">
          <span><b>{kindCounts.article}</b> 文章</span>
          <span className="inbox-stats__sep" aria-hidden="true">·</span>
          <span><b>{kindCounts.note}</b> 随记</span>
          <span className="inbox-stats__sep" aria-hidden="true">·</span>
          <span><b>{counts.unread}</b> 未读</span>
        </span>
        <SyncControl disabled={model.adding || model.batch.running} onSynced={model.refresh} inHeader />
      </header>

      <form
        className={`inbox-add${model.adding ? ' is-busy' : ''}${draft.trim() ? (isLinkDraft ? ' is-link' : ' is-note') : ''}`}
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <span className="inbox-add__icon" aria-hidden="true">
          <Icon name={isLinkDraft ? 'link' : 'capture'} />
        </span>
        <input
          ref={inputRef}
          type="text"
          autoComplete="off"
          spellCheck={false}
          name="inbox"
          placeholder={narrow ? '粘贴链接或打字记随记' : '粘贴链接存文章，打字记随记'}
          aria-label="收件箱输入：链接存文章，文本记随记"
          value={draft}
          disabled={model.adding}
          onChange={(event) => {
            setDraft(event.target.value);
            if (model.addError) model.clearAddError();
          }}
        />
        <button type="submit" className="solid-action" disabled={model.adding || !draft.trim()}>
          {model.adding ? '处理中…' : isLinkDraft ? '添加文章' : '存入随记'}
        </button>
        <div className="inbox-add__foot">
          <span
            className={`inbox-hint${draft.trim() ? (isLinkDraft ? ' is-link' : ' is-note') : ''}`}
            style={{ visibility: draft.trim() ? 'visible' : 'hidden' }}
            role="status"
          >
            {isLinkDraft ? '检测到链接 → 存为文章，正文自动抓取解析' : '直接打字 → 存入随记'}
          </span>
          <span className="inbox-add__spacer" />
          {!isMobilePlatform && (
            <button type="button" className="inbox-add__tool" disabled={model.adding || model.batch.running} data-tip="从浏览器保存当前页——抓取浏览器当前标签页的正文，解析后存为文章（桌面端功能）。" onClick={() => void importCurrentBrowserPage()}>
              <Icon name="external" /> 浏览器保存
            </button>
          )}
          <button type="button" className="inbox-add__tool" disabled={model.adding || model.batch.running} data-tip="打开本地文件导入——支持 HTML / Markdown / 纯文本；在文件管理器里「打开方式」选 Virgo 同效，都落进收件箱。" onClick={() => void openLocalFile()}>
            <Icon name="folder" /> 打开文件
          </button>
          <button type="button" className="inbox-add__tool" disabled={model.adding || model.batch.running} data-tip="批量导入——粘贴多行链接一次存为多篇，自动忽略链接之外的文字，支持微信公众号与 X（推特）链接。" onClick={() => setBatchOpen(true)}>
            <Icon name="download" /> 批量导入
          </button>
        </div>
        {model.addError && (
          <p className="reading-add__error inbox-add__alert" role="alert">{model.addError}</p>
        )}
        {model.addNotice && (
          <p className="reading-add__status" role="status">{model.addNotice}</p>
        )}
      </form>

      {batchOpen && (
        <div className="reading-batch" aria-label="批量导入">
          <textarea
            value={batchDraft}
            onChange={(event) => setBatchDraft(event.target.value)}
            placeholder="粘贴包含链接的内容，例如聊天记录或笔记。链接之外的文字会被忽略。"
            aria-label="批量导入内容"
            rows={5}
            disabled={model.batch.running}
          />
          <div className="reading-batch__row" role="group" aria-label="导入范围">
            {(Object.keys(BATCH_SCOPE_LABEL) as BatchScope[]).map((scope) => (
              <button
                type="button"
                key={scope}
                className={`due-summary__chip${batchScope === scope ? ' is-active' : ''}`}
                aria-pressed={batchScope === scope}
                disabled={model.batch.running}
                onClick={() => setBatchScope(scope)}
              >
                <span>{BATCH_SCOPE_LABEL[scope]}</span>
                <strong>{extractBatchUrls(batchDraft, scope).length}</strong>
              </button>
            ))}
            <span className="reading-batch__hint">全部类型同时支持普通网页文章</span>
            <span className="reading-batch__spacer" />
            <button
              type="button"
              disabled={batchUrls.length === 0 || model.batch.running}
              onClick={() => void model.addBatch(batchUrls).then(() => {
                setBatchDraft('');
                setBatchOpen(false);
              })}
            >
              {model.batch.running ? '正在放入列表…' : '导入'}
            </button>
            <button type="button" className="reading-add__browser" onClick={() => setBatchOpen(false)}>
              关闭
            </button>
          </div>
          {model.batch.total > 0 && !model.batch.running && (
            <p className="reading-add__status" role="status">
              已放入 {model.batch.ok} 条，正文在后台解析；可直接继续添加。
              {model.batch.failed > 0 && ` 另有 ${model.batch.failed} 条链接无法识别。`}
            </p>
          )}
          {model.batchErrors.length > 0 && (
            <ul className="reading-batch__errors" role="list">
              {model.batchErrors.slice(-5).map((failure) => (
                <li key={failure.url} title={failure.url}>
                  <span>{failure.url}</span> — {failure.message}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <div className="reading-toolbar">
        <div className="due-summary reading-filters">
          <div className="due-summary" role="group" aria-label="按类型筛选">
            {KIND_ORDER.map((kind) => (
              <button
                type="button"
                key={kind}
                className={`due-summary__chip${model.kindFilter === kind ? ' is-active' : ''}`}
                aria-pressed={model.kindFilter === kind}
                onClick={() => model.setKindFilter(kind)}
              >
                <span>{KIND_LABEL[kind]}</span>
                <strong>{countForKind(kind)}</strong>
              </button>
            ))}
          </div>
          <span className="reading-filters__divider" aria-hidden="true" />
          <div className="due-summary" role="group" aria-label="按阅读状态筛选">
            {FILTER_ORDER.map((filter) => (
              <button
                type="button"
                key={filter}
                className={`due-summary__chip${model.filter === filter ? ' is-active' : ''}`}
                aria-pressed={model.filter === filter}
                onClick={() => model.setFilter(filter)}
              >
                <span>{READING_FILTER_LABEL[filter]}</span>
                <strong>{countFor(filter)}</strong>
              </button>
            ))}
          </div>
          <span className="reading-toolbar__spacer" />
          <button
            type="button"
            className="text-action"
            onClick={onOpenTriage}
            title="随记分拣：把收件箱里的随记转成项目任务，原话留档"
          >
            随记分拣 <Icon name="arrow" />
          </button>
        </div>
      </div>

      {clipboard && (
        <div className="reading-clipboard" role="status" aria-label="剪贴板链接">
          <span className="reading-clipboard__icon" aria-hidden="true"><Icon name="link" /></span>
          <span className="reading-clipboard__copy">
            {clipboard.urls.length === 1 ? (
              <>
                剪贴板里有链接 <strong>{hostOf(clipboard.urls[0])}</strong>，要导入吗？
              </>
            ) : (
              <>剪贴板里有 {clipboard.urls.length} 条链接，要全部导入吗？</>
            )}
          </span>
          <span className="reading-clipboard__actions">
            <button type="button" className="text-action text-action--primary" onClick={() => void importClipboard()}>
              {clipboard.urls.length === 1 ? '导入' : '批量导入'}
            </button>
            <button type="button" className="text-action" onClick={dismissClipboard}>
              忽略
            </button>
          </span>
        </div>
      )}

      {model.error && <p className="reading-notice" role="alert">{model.error}</p>}

      {exportNotice && (
        <p
          className={`reading-notice${exportNotice.kind === 'error' ? '' : ' reading-notice--ok'}${exportNotice.path ? ' reading-notice--actions' : ''}`}
          role={exportNotice.kind === 'error' ? 'alert' : 'status'}
          data-testid="reading-export-notice"
        >
          <span className="reading-notice__message">{exportNotice.message}</span>
          {exportNotice.path && (
            <button type="button" onClick={() => void openExportedFile(exportNotice, reportExportNotice)}>
              <Icon name="external" /> 打开
            </button>
          )}
          {exportNotice.dir && (
            <button type="button" onClick={() => void openExportedDir(exportNotice, reportExportNotice)}>
              <Icon name="folder" /> 打开目录
            </button>
          )}
        </p>
      )}

      {model.loading ? (
        <p className="ai-loading" role="status">正在读取收件箱…</p>
      ) : model.articles.length === 0 ? (
        <div className="reading-empty">
          <h2>收件箱还是空的</h2>
          <p>粘贴链接存文章，直接打字记随记；文件管理器里「打开方式」选 Virgo 的内容也落在这里。</p>
        </div>
      ) : model.visible.length === 0 ? (
        <div className="reading-empty">
          <h2>这个筛选下没有内容</h2>
          <p>换一个类型或状态看看，或者从上面的输入框添加。</p>
        </div>
      ) : (
        <div className="reading-list" role="list" aria-label="收件箱条目">
          {model.visible.map((article) =>
            article.kind === 'note' ? (
              <NoteCard
                key={article.id}
                article={article}
                busy={model.busy.has(article.id)}
                onEdit={(id) => void editNote(id)}
                onRemove={(id) => void model.remove(id)}
              />
            ) : (
              <ReadingCard
                key={article.id}
                article={article}
                busy={model.busy.has(article.id)}
                opening={model.opening === article.id}
                onOpen={(id) => void model.openArticle(id)}
                onSetRead={(id, read) => void model.setRead(id, read)}
                onRemove={(id) => void model.remove(id)}
                onRetry={(id) => void model.retryParse(id)}
                onExportMenu={(id, position) => {
                  if (exportBusy) return;
                  setExportMenu({ id, ...position });
                }}
              />
            ),
          )}
        </div>
      )}
      {noteEdit && (
        <NoteDrawer
          draft={noteEdit.markdown}
          saving={noteSaving}
          lastEdited={noteEdit.lastEdited}
          onChange={(markdown) => setNoteEdit((current) => (current ? { ...current, markdown } : current))}
          onSave={() => void saveNote()}
          onDelete={() => void removeNote()}
          onClose={() => setNoteEdit(null)}
        />
      )}
      {exportMenu && (
        <ReaderContextMenu
          x={exportMenu.x}
          y={exportMenu.y}
          onExport={(format) => void exportArticle(format)}
          onClose={() => setExportMenu(null)}
        />
      )}
    </section>
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

/// 与 todo-sync 服务器（todo-sync.apikv.com）对账。配置存 localStorage，
/// 与网页端 sync-web 的键一致，方便同一份 token 两处复用。
///
/// 有配置时自动运行：Rust 侧保持 `/stream` 长连接，连上先补推离线期间攒下的
/// 条目，之后每次服务器变更自动对账；断线退避重试。这里只负责显示状态和
/// 手动触发一次。A′ 定稿后头部用状态芯片形态（inHeader）：teal 圆点 +
/// 状态文案，点击展开同一套配置面板。
function SyncControl({ disabled, onSynced, inHeader = false }: { disabled: boolean; onSynced: () => void; inHeader?: boolean }) {
  const [open, setOpen] = React.useState(false);
  const stored = React.useMemo(() => readSyncConfig(), []);
  const [base, setBase] = React.useState(stored?.base ?? 'https://todo-sync.apikv.com');
  const [token, setToken] = React.useState(stored?.token ?? '');
  const [busy, setBusy] = React.useState(false);
  const [progress, setProgress] = React.useState<SyncProgress | null>(null);
  const [report, setReport] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [link, setLink] = React.useState<LinkStatus>({ state: 'idle', retry_in: 0, error: null });
  const onSyncedRef = React.useRef(onSynced);
  onSyncedRef.current = onSynced;

  React.useEffect(() => {
    if (!isTauriRuntime()) return;
    let stop: (() => void) | undefined;
    void onSyncEvents({
      onLink: (status) => {
        setLink(status);
        if (status.state === 'offline' && status.error) setError(status.error);
        if (status.state === 'online') setError(null);
      },
      onStart: () => {
        setBusy(true);
        setProgress(null);
      },
      onProgress: (next) => setProgress(next),
      onSynced: (next) => {
        setBusy(false);
        setProgress(null);
        setError(null);
        setReport(formatSyncReport(next));
        // 只有真的拉到了新条目才刷新列表，避免每次心跳都重排卡片。
        if (next.pulled > 0) onSyncedRef.current();
      },
      onError: (message) => {
        setBusy(false);
        setProgress(null);
        setError(message);
      },
    }).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop?.();
  }, []);

  // 已有配置时恢复自动同步（App 启动时也会启动，这里是兜底）。
  React.useEffect(() => {
    const config = readSyncConfig();
    if (!config) return;
    void startSyncWatcher(config).catch((cause) => setError(String(cause)));
  }, []);

  if (!isTauriRuntime()) return null;

  const run = async () => {
    if (busy || !base.trim() || !token.trim()) return;
    setBusy(true);
    setError(null);
    const config = { base: base.trim(), token: token.trim() };
    try {
      const result = await call<SyncReport>('reading_sync_now', config);
      writeSyncConfig(config);
      setReport(formatSyncReport(result));
      onSynced();
      // 新配置（可能是刚换的 token）立刻接管常驻连接。
      await startSyncWatcher(config).catch(() => {});
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    clearSyncConfig();
    await stopSyncWatcher().catch(() => {});
    setLink({ state: 'idle', retry_in: 0, error: null });
    setReport(null);
    setError(null);
  };

  const headerChip = () => {
    if (busy) return { text: '同步中…', state: 'busy' as const };
    switch (link.state) {
      case 'online':
        return { text: '已同步', state: 'online' as const };
      case 'connecting':
        return { text: '连接中…', state: 'busy' as const };
      case 'offline':
        return { text: '离线', state: 'offline' as const };
      default:
        return { text: '同步', state: 'idle' as const };
    }
  };
  const chip = inHeader ? headerChip() : null;

  return (
    <span className="reading-sync">
      <button
        type="button"
        className={inHeader ? 'inbox-sync' : 'reading-add__browser'}
        disabled={disabled || busy}
        title={link.state === 'idle' ? '与 todo-sync 服务器双向对账' : linkLabel(link)}
        onClick={() => setOpen((current) => !current)}
      >
        {chip && <i className="inbox-sync__dot" data-state={chip.state} aria-hidden="true" />}
        {chip ? chip.text : busy ? '同步中…' : '同步'}
      </button>
      {open && (
        <span className="reading-sync__panel">
          <label>
            服务器
            <input value={base} onChange={(event) => setBase(event.target.value)} placeholder="https://todo-sync.apikv.com" />
          </label>
          <label>
            Token
            <input type="password" value={token} onChange={(event) => setToken(event.target.value)} placeholder="todo-sync-token 的值" />
          </label>
          <span className="reading-sync__actions">
            <button type="button" disabled={busy || !base.trim() || !token.trim()} onClick={() => void run()}>
              立即对账
            </button>
            {busy && (
              <button type="button" className="text-action" onClick={() => void cancelSync()}>
                取消
              </button>
            )}
            {link.state !== 'idle' && (
              <button type="button" className="text-action" onClick={() => void disconnect()}>
                断开自动同步
              </button>
            )}
          </span>
          <span className="reading-sync__state" data-state={link.state}>
            {linkLabel(link)}
          </span>
          {busy && progress && (
            <span className="reading-sync__report" role="status">{formatSyncProgress(progress)}</span>
          )}
          {report && <span className="reading-sync__report" role="status">{report}</span>}
          {error && <span className="reading-sync__error" role="alert">{error}</span>}
        </span>
      )}
    </span>
  );
}
