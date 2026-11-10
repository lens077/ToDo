import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import {
  getArticle,
  listArticles,
  refetch,
  removeArticle,
  saveLink,
  setRead,
  subscribeChanges,
  updateNotes,
  type ReadingArticle,
  type ReadingArticleDetail,
  type SyncConfig,
} from './api';

const CONFIG_KEY = 'virgo-sync-config';

function loadConfig(): SyncConfig | null {
  try {
    const raw = localStorage.getItem(CONFIG_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<SyncConfig>;
    if (parsed.token && parsed.base) return { base: parsed.base, token: parsed.token };
  } catch {
    // 落回配置表单。
  }
  return null;
}

function defaultBase(): string {
  return window.location.origin;
}

function timeLabel(ms: number): string {
  const date = new Date(ms);
  const now = Date.now();
  const days = (now - ms) / 86_400_000;
  if (days < 1) return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  if (days < 7) return `${Math.floor(days)} 天前`;
  return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

function isRead(article: ReadingArticle): boolean {
  return article.read_at !== null || article.progress_percent === 100;
}

type Filter = 'all' | 'unread' | 'read';

function SetupScreen({ onReady }: { onReady: (config: SyncConfig) => void }) {
  const [base, setBase] = useState(defaultBase());
  const [token, setToken] = useState('');
  return (
    <div className="setup">
      <h1><strong>Virgo</strong> Sync</h1>
      <p className="setup__hint">链接库自托管服务。Token 在 k8s Secret（todo-sync-token）里，粘一次存本机。</p>
      <label>服务地址<input value={base} onChange={(event) => setBase(event.target.value)} /></label>
      <label>Token<input type="password" value={token} onChange={(event) => setToken(event.target.value)} autoFocus /></label>
      <button
        type="button"
        disabled={!token.trim()}
        onClick={() => onReady({ base: base.replace(/\/$/, ''), token: token.trim() })}
      >
        进入
      </button>
    </div>
  );
}

function SaveBox({ config, onSaved }: { config: SyncConfig; onSaved: () => void }) {
  const [url, setUrl] = useState('');
  const [notes, setNotes] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = useCallback(async () => {
    const target = url.trim();
    if (!target || busy) return;
    setBusy(true);
    setError(null);
    try {
      await saveLink(config, target, notes.trim() || undefined);
      setUrl('');
      setNotes('');
      onSaved();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [busy, config, notes, onSaved, url]);

  return (
    <section className="savebox">
      <div className="savebox__row">
        <input
          className="savebox__url"
          placeholder="粘贴链接（x.com / 微信 / 普通网页），保存后后台解析"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          onKeyDown={() => void submit()}
        />
        <button type="button" disabled={busy || !url.trim()} onClick={() => void submit()}>
          {busy ? '保存中…' : '保存'}
        </button>
      </div>
      <input
        className="savebox__notes"
        placeholder="备注（可选，保存时记录当时的想法）"
        value={notes}
        onChange={(event) => setNotes(event.target.value)}
        onKeyDown={() => void submit()}
      />
      {error && <p className="savebox__error">{error}</p>}
    </section>
  );
}

function ArticleRow({ article, onOpen }: { article: ReadingArticle; onOpen: () => void }) {
  return (
    <li className="row" onClick={onOpen}>
      <div className="row__main">
        <span className="row__title">{article.title || article.url}</span>
        {article.excerpt && <span className="row__excerpt">{article.excerpt}</span>}
        <span className="row__meta">
          {article.site_name && <span>{article.site_name}</span>}
          {article.word_count > 0 && <span>{article.reading_minutes} 分钟</span>}
          <span>{timeLabel(article.added_at)}</span>
        </span>
      </div>
      <span className={`row__state ${isRead(article) ? 'is-read' : ''}`}>
        {isRead(article) ? '已读' : '未读'}
      </span>
    </li>
  );
}

function DetailPane({
  config,
  id,
  onBack,
  onChanged,
}: {
  config: SyncConfig;
  id: string;
  onBack: () => void;
  onChanged: () => void;
}) {
  const [detail, setDetail] = useState<ReadingArticleDetail | null>(null);
  const [notes, setNotes] = useState('');
  const [error, setError] = useState<string | null>(null);
  const savedNotes = useRef('');

  useEffect(() => {
    let alive = true;
    setDetail(null);
    void getArticle(config, id)
      .then((loaded) => {
        if (!alive) return;
        setDetail(loaded);
        setNotes(loaded.context.personal_notes);
        savedNotes.current = loaded.context.personal_notes;
      })
      .catch((cause) => alive && setError(String(cause)));
    return () => {
      alive = false;
    };
  }, [config, id]);

  const saveNotes = useCallback(async () => {
    if (!detail || notes === savedNotes.current) return;
    savedNotes.current = notes;
    try {
      const updated = await updateNotes(config, detail.id, notes);
      setDetail(updated);
      onChanged();
    } catch (cause) {
      setError(String(cause));
    }
  }, [config, detail, notes, onChanged]);

  if (error) {
    return <div className="detail"><button type="button" onClick={onBack}>← 返回</button><p className="savebox__error">{error}</p></div>;
  }
  if (!detail) return <div className="detail"><button type="button" onClick={onBack}>← 返回</button><p>加载中…</p></div>;

  return (
    <article className="detail">
      <header className="detail__head">
        <button type="button" onClick={onBack}>← 返回</button>
        <div className="detail__actions">
          {!isRead(detail) && (
            <button type="button" onClick={() => void setRead(config, detail.id, true).then(() => { onChanged(); void getArticle(config, id).then(setDetail); })}>
              标记已读
            </button>
          )}
          <button type="button" onClick={() => void refetch(config, detail.id).then(() => { onChanged(); })}>
            重新解析
          </button>
          <button
            type="button"
            className="danger"
            onClick={() => void removeArticle(config, detail.id).then(() => { onChanged(); onBack(); })}
          >
            删除
          </button>
        </div>
      </header>
      <h1>{detail.title || detail.url}</h1>
      <p className="detail__meta">
        {detail.byline && <span>{detail.byline} · </span>}
        {detail.site_name && <span>{detail.site_name} · </span>}
        <a href={detail.url} target="_blank" rel="noreferrer">打开原文</a>
        {detail.context.parse_status === 'failed' && <span className="detail__failed"> · 解析失败，可打开原文或点「重新解析」</span>}
      </p>
      <section className="detail__notes">
        <label>我的备注<textarea
          value={notes}
          placeholder="保存时想记的话：为什么收藏、关注什么……"
          onChange={(event) => setNotes(event.target.value)}
          onBlur={() => void saveNotes()}
        /></label>
      </section>
      {detail.content_html
        ? <section className="detail__content" dangerouslySetInnerHTML={{ __html: detail.content_html }} />
        : <p className="detail__empty">正文尚未解析完成，稍后点「重新解析」。</p>}
      {detail.annotations.length > 0 && (
        <section className="detail__annos">
          <h2>圈选笔记</h2>
          <ul>{detail.annotations.map((anno) => (
            <li key={anno.id}><blockquote>{anno.quote}</blockquote><p>{anno.note}</p></li>
          ))}</ul>
        </section>
      )}
    </article>
  );
}

export function App(): ReactNode {
  const [config, setConfig] = useState<SyncConfig | null>(loadConfig);
  const [articles, setArticles] = useState<ReadingArticle[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [openId, setOpenId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!config) return;
    try {
      const { articles: fresh } = await listArticles(config);
      setArticles(fresh);
      setError(null);
    } catch (cause) {
      setError(String(cause));
    }
  }, [config]);

  useEffect(() => {
    if (!config) return;
    void refresh();
    return subscribeChanges(config, () => void refresh());
  }, [config, refresh]);

  const visible = useMemo(() => {
    if (!articles) return [];
    const keyword = query.trim().toLowerCase();
    return articles
      .filter((article) => (filter === 'unread' ? !isRead(article) : filter === 'read' ? isRead(article) : true))
      .filter((article) =>
        keyword
          ? `${article.title} ${article.url} ${article.excerpt} ${article.site_name}`.toLowerCase().includes(keyword)
          : true)
      .sort((left, right) => right.added_at - left.added_at);
  }, [articles, filter, query]);

  if (!config) {
    return <SetupScreen onReady={(next) => { localStorage.setItem(CONFIG_KEY, JSON.stringify(next)); setConfig(next); }} />;
  }

  return (
    <div className="app">
      <header className="topbar">
        <h1><strong>Virgo</strong> Sync</h1>
        <span className={`topbar__live ${error ? '' : 'is-live'}`} title={error ? '连接异常' : '已连接'}>{error ? '○' : '●'}</span>
        <button type="button" className="topbar__logout" onClick={() => { localStorage.removeItem(CONFIG_KEY); setConfig(null); }}>
          换 Token
        </button>
      </header>

      {openId ? (
        <DetailPane config={config} id={openId} onBack={() => setOpenId(null)} onChanged={() => void refresh()} />
      ) : (
        <main className="main">
          <SaveBox config={config} onSaved={() => void refresh()} />
          <div className="toolbar">
            <input
              className="toolbar__search"
              placeholder="搜索标题 / 链接 / 摘要 / 站点"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            {(['all', 'unread', 'read'] as const).map((value) => (
              <button key={value} type="button" className={filter === value ? 'is-active' : ''} onClick={() => setFilter(value)}>
                {{ all: '全部', unread: '未读', read: '已读' }[value]}
              </button>
            ))}
          </div>
          {error && <p className="savebox__error">{error}</p>}
          {articles === null ? (
            <p className="main__hint">加载中…</p>
          ) : visible.length === 0 ? (
            <p className="main__hint">没有匹配的链接。粘贴一个网址开始收藏。</p>
          ) : (
            <ul className="list">
              {visible.map((article) => (
                <ArticleRow key={article.id} article={article} onOpen={() => setOpenId(article.id)} />
              ))}
            </ul>
          )}
        </main>
      )}
    </div>
  );
}
