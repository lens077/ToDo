import * as React from 'react';
import type { ReadingModel } from '../hooks/useReading';
import { absoluteTime, relativeTime } from '../lib/ai';
import {
  BATCH_SCOPE_LABEL,
  extractBatchUrls,
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
import {
  clearSyncConfig,
  formatSyncReport,
  isTauriRuntime,
  linkLabel,
  onSyncEvents,
  readSyncConfig,
  startSyncWatcher,
  stopSyncWatcher,
  writeSyncConfig,
  type LinkStatus,
  type SyncReport,
} from '../lib/readingSync';

interface ReadingViewProps {
  model: ReadingModel;
}

const FILTER_ORDER: ReadingFilter[] = ['all', 'unread', 'reading', 'read'];

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
}

function ReadingCard({ article, busy, opening, onOpen, onSetRead, onRemove }: ReadingCardProps) {
  const state = readingState(article);
  const percent = state === 'read' ? 100 : article.progress_percent;
  const action =
    state === 'read' ? '重读' : state === 'reading' ? `继续阅读 · ${article.progress_percent}%` : '开始阅读';
  return (
    <article
      className={`reading-card reading-card--${state}${busy ? ' is-busy' : ''}`}
      data-testid="reading-card"
      data-article={article.id}
      aria-label={article.title}
    >
      <button type="button" className="reading-card__open" onClick={() => onOpen(article.id)} disabled={busy || opening} aria-label={`打开：${article.title}`}>
        <Cover article={article} />
        <span className="reading-card__body">
          <span className="reading-card__title">{article.title}</span>
          {article.excerpt && <span className="reading-card__excerpt">{article.excerpt}</span>}
          <span className="reading-card__meta">
            <span title={article.url}>{article.site_name || hostOf(article.url)}</span>
            <span aria-hidden="true">·</span>
            <span>{article.reading_minutes} 分钟</span>
            <span aria-hidden="true">·</span>
            <time dateTime={new Date(article.added_at).toISOString()} title={absoluteTime(article.added_at)}>
              {relativeTime(article.added_at)}
            </time>
            {article.annotation_count > 0 && (
              <>
                <span aria-hidden="true">·</span>
                <span className="reading-card__notes"><Icon name="capture" />{article.annotation_count} 条笔记</span>
              </>
            )}
          </span>
        </span>
      </button>
      <div className="reading-card__side">
        {state === 'read' ? (
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
            disabled={busy}
            title={state === 'read' ? '标为未读' : '标为已读'}
            aria-label={state === 'read' ? `标为未读：${article.title}` : `标为已读：${article.title}`}
            onClick={() => onSetRead(article.id, state !== 'read')}
          >
            <Icon name={state === 'read' ? 'refresh' : 'check'} />
          </button>
          <button
            type="button"
            className="icon-action"
            disabled={busy}
            title="删除（同时删除本地正文与笔记）"
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

export function ReadingView({ model }: ReadingViewProps) {
  const [draft, setDraft] = React.useState('');
  const inputRef = React.useRef<HTMLInputElement>(null);
  const [batchOpen, setBatchOpen] = React.useState(false);
  const [batchScope, setBatchScope] = React.useState<BatchScope>('all');
  const [batchDraft, setBatchDraft] = React.useState('');
  const counts = React.useMemo(() => countByState(model.articles), [model.articles]);
  const batchUrls = React.useMemo(() => extractBatchUrls(batchDraft, batchScope), [batchDraft, batchScope]);

  const submit = async () => {
    if (model.adding || model.batch.running || !draft.trim()) return;
    if (await model.add(draft)) {
      setDraft('');
      inputRef.current?.focus();
    }
  };

  const countFor = (filter: ReadingFilter) => (filter === 'all' ? model.articles.length : counts[filter]);

  const importCurrentBrowserPage = async () => {
    const url = window.prompt('粘贴浏览器当前页面的 URL');
    if (!url) return;
    const html = window.prompt('粘贴浏览器复制的页面 HTML（开发者工具中复制 outerHTML）');
    if (!html) return;
    await model.importBrowserPage(url, html);
  };

  return (
    <section className="reading-view" aria-labelledby="reading-title">
      <header className="view-heading view-heading--reading">
        <div className="view-heading__title">
          <h1 id="reading-title">阅读</h1>
          <p className="view-heading__meta">
            <span>{model.articles.length} 篇文章</span>
            <span aria-hidden="true">·</span>
            <span>{counts.reading} 篇读到一半</span>
            <span aria-hidden="true">·</span>
            <span>{counts.read} 篇已读</span>
          </p>
        </div>
        <p className="view-heading__note">粘贴任意链接，正文与图片会保存到本机；标记与笔记只记在本机，不写入项目。</p>
      </header>

      <form
        className={`reading-add${model.adding ? ' is-busy' : ''}`}
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <span className="reading-add__icon" aria-hidden="true"><Icon name="link" /></span>
        <input
          ref={inputRef}
          type="text"
          inputMode="url"
          autoComplete="off"
          spellCheck={false}
          name="url"
          placeholder="粘贴链接，支持微信公众号文章、X（推特）链接…"
          aria-label="要添加的链接"
          value={draft}
          disabled={model.adding}
          onChange={(event) => {
            setDraft(event.target.value);
            if (model.addError) model.clearAddError();
          }}
        />
        {model.adding ? (
          <button type="button" onClick={model.cancelAdd}>
            取消
          </button>
        ) : (
          <button type="submit" disabled={!draft.trim()}>
            添加
          </button>
        )}
        <span className="reading-add__actions">
          <button type="button" className="reading-add__browser" disabled={model.adding || model.batch.running} onClick={() => void importCurrentBrowserPage()}>
            从浏览器保存
          </button>
          <button type="button" className="reading-add__browser" disabled={model.adding || model.batch.running} onClick={() => setBatchOpen(true)}>
            批量导入
          </button>
          <SyncControl disabled={model.adding || model.batch.running} onSynced={model.refresh} />
        </span>
        {model.addError && (
          <p className="reading-add__error" role="alert">{model.addError}</p>
        )}
        {model.addNotice && (
          <p className="reading-add__status" role="status">{model.addNotice}</p>
        )}
        {model.adding && (
          <div className="reading-preview reading-preview--loading" role="status">
            <span className="reading-preview__thumb reading-preview__thumb--loading"><Icon name="capture" /></span>
            <span className="reading-preview__copy"><strong>正在抓取正文…</strong><span>标题、正文与图片会保存到本机</span></span>
            <span className="reading-preview__tools">
              <button type="button" className="text-action" onClick={model.cancelAdd}>取消等待</button>
              <span className="reading-preview__spinner" aria-hidden="true" />
            </span>
          </div>
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
            {model.batch.running ? (
              <button type="button" onClick={model.cancelBatch}>
                取消
              </button>
            ) : (
              <>
                <button
                  type="button"
                  disabled={batchUrls.length === 0}
                  onClick={() => void model.addBatch(batchUrls)}
                >
                  开始导入
                </button>
                <button type="button" className="reading-add__browser" onClick={() => setBatchOpen(false)}>
                  关闭
                </button>
              </>
            )}
          </div>
          {model.batch.running && (
            <p className="reading-add__status" role="status">
              正在导入 {model.batch.done + 1}/{model.batch.total} · 成功 {model.batch.ok} · 失败 {model.batch.failed}
              {model.batch.current && <span title={model.batch.current}>（{hostOf(model.batch.current)}）</span>}
            </p>
          )}
          {!model.batch.running && model.batch.total > 0 && (
            <p className="reading-add__status" role="status">
              {model.batch.done < model.batch.total ? '已取消。' : '导入完成。'}成功 {model.batch.ok} · 失败 {model.batch.failed}
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
        <div className="due-summary reading-filters" role="group" aria-label="按阅读状态筛选">
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
      </div>

      {model.error && <p className="ai-notice" role="alert">{model.error}</p>}

      {model.loading ? (
        <p className="ai-loading" role="status">正在读取阅读库…</p>
      ) : model.articles.length === 0 ? (
        <div className="reading-empty">
          <h2>还没有保存任何文章</h2>
          <p>把想读的链接贴到上面。正文会以可读的形式保存到本机，之后即使原文删除也能继续阅读。</p>
        </div>
      ) : model.visible.length === 0 ? (
        <div className="reading-empty">
          <h2>这个筛选下没有文章</h2>
          <p>换一个状态看看，或者添加新的链接。</p>
        </div>
      ) : (
        <div className="reading-list" role="list" aria-label="已保存的文章">
          {model.visible.map((article) => (
            <ReadingCard
              key={article.id}
              article={article}
              busy={model.busy.has(article.id)}
              opening={model.opening === article.id}
              onOpen={(id) => void model.openArticle(id)}
              onSetRead={(id, read) => void model.setRead(id, read)}
              onRemove={(id) => void model.remove(id)}
            />
          ))}
        </div>
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
/// 手动触发一次。
function SyncControl({ disabled, onSynced }: { disabled: boolean; onSynced: () => void }) {
  const [open, setOpen] = React.useState(false);
  const stored = React.useMemo(() => readSyncConfig(), []);
  const [base, setBase] = React.useState(stored?.base ?? 'https://todo-sync.apikv.com');
  const [token, setToken] = React.useState(stored?.token ?? '');
  const [busy, setBusy] = React.useState(false);
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
      onStart: () => setBusy(true),
      onSynced: (next) => {
        setBusy(false);
        setError(null);
        setReport(formatSyncReport(next));
        // 只有真的拉到了新条目才刷新列表，避免每次心跳都重排卡片。
        if (next.pulled > 0 || next.refetched > 0) onSyncedRef.current();
      },
      onError: (message) => {
        setBusy(false);
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

  return (
    <span className="reading-sync">
      <button
        type="button"
        className="reading-add__browser"
        disabled={disabled || busy}
        title={link.state === 'idle' ? '与 todo-sync 服务器双向对账' : linkLabel(link)}
        onClick={() => setOpen((current) => !current)}
      >
        {busy ? '同步中…' : '同步'}
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
            {link.state !== 'idle' && (
              <button type="button" className="text-action" onClick={() => void disconnect()}>
                断开自动同步
              </button>
            )}
          </span>
          <span className="reading-sync__state" data-state={link.state}>
            {linkLabel(link)}
          </span>
          {report && <span className="reading-sync__report" role="status">{report}</span>}
          {error && <span className="reading-sync__error" role="alert">{error}</span>}
        </span>
      )}
    </span>
  );
}
