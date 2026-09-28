import * as React from 'react';
import type { ReadingModel } from '../hooks/useReading';
import { absoluteTime, relativeTime } from '../lib/ai';
import {
  READING_FILTER_LABEL,
  countByState,
  coverSource,
  hostOf,
  readingState,
  type ReadingArticle,
  type ReadingFilter,
} from '../lib/reading';
import { Icon } from './Icon';

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
  const counts = React.useMemo(() => countByState(model.articles), [model.articles]);

  const submit = async () => {
    if (model.adding || !draft.trim()) return;
    if (await model.add(draft)) {
      setDraft('');
      inputRef.current?.focus();
    }
  };

  const countFor = (filter: ReadingFilter) => (filter === 'all' ? model.articles.length : counts[filter]);

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
          placeholder="粘贴链接，例如微信公众号文章…"
          aria-label="要添加的链接"
          value={draft}
          disabled={model.adding}
          onChange={(event) => {
            setDraft(event.target.value);
            if (model.addError) model.clearAddError();
          }}
        />
        <button type="submit" disabled={model.adding || !draft.trim()}>
          {model.adding ? '抓取中…' : '添加'}
        </button>
        {model.addError && (
          <p className="reading-add__error" role="alert">{model.addError}</p>
        )}
        {model.adding && (
          <div className="reading-preview reading-preview--loading" role="status">
            <span className="reading-preview__thumb reading-preview__thumb--loading"><Icon name="capture" /></span>
            <span className="reading-preview__copy"><strong>正在抓取正文…</strong><span>标题、正文与图片会保存到本机</span></span>
            <span className="reading-preview__spinner" aria-hidden="true" />
          </div>
        )}
      </form>

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
