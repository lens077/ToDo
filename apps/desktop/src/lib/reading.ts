/**
 * 阅读: the personal read-later library.
 *
 * Articles are fetched and stored by the Rust side (`todo_core::ReadingStore`)
 * under `~/.todo/reading/`; they belong to the user, not to any project. The
 * browser build has no such store, so the mock keeps a few sample articles in
 * memory for the e2e suite and `todo serve` reports the feature unavailable.
 *
 * Reading position follows Readeck / Karakeep: `progress_percent` for the
 * list, `progress_anchor` (the block the viewport was on) to restore the exact
 * place. 100% is read and sticky.
 */
import { convertFileSrc, invoke } from '@tauri-apps/api/core';

export interface ReadingArticle {
  id: string;
  url: string;
  title: string;
  site_name: string;
  excerpt: string;
  byline: string | null;
  cover_path: string | null;
  word_count: number;
  reading_minutes: number;
  added_at: number;
  last_opened_at: number | null;
  progress_percent: number;
  progress_anchor: string | null;
  read_at: number | null;
  annotation_count: number;
}

export interface ReadingAnnotation {
  id: string;
  article_id: string;
  quote: string;
  start: number;
  end: number;
  note: string;
  created_at: number;
  updated_at: number;
}

export interface ReadingArticleDetail extends ReadingArticle {
  content_html: string;
  markdown: string | null;
  image_dir: string;
  annotations: ReadingAnnotation[];
}

export interface AnnotationInput {
  id: string;
  article_id: string;
  quote: string;
  start: number;
  end: number;
  note: string;
}

export type ReadingFilter = 'all' | 'unread' | 'reading' | 'read';

export const READING_FILTER_LABEL: Record<ReadingFilter, string> = {
  all: '全部',
  unread: '未读',
  reading: '进行中',
  read: '已读',
};

export type ReadingState = 'unread' | 'reading' | 'read';

export function readingState(article: Pick<ReadingArticle, 'progress_percent' | 'read_at'>): ReadingState {
  if (article.read_at !== null || article.progress_percent >= 100) return 'read';
  if (article.progress_percent > 0) return 'reading';
  return 'unread';
}

export function filterArticles(articles: ReadingArticle[], filter: ReadingFilter): ReadingArticle[] {
  if (filter === 'all') return articles;
  return articles.filter((article) => readingState(article) === filter);
}

export function countByState(articles: ReadingArticle[]): Record<ReadingState, number> {
  const counts: Record<ReadingState, number> = { unread: 0, reading: 0, read: 0 };
  for (const article of articles) counts[readingState(article)] += 1;
  return counts;
}

/** Articles the sidebar badge counts: everything not yet finished. */
export function unfinishedCount(articles: ReadingArticle[]): number {
  return articles.filter((article) => readingState(article) !== 'read').length;
}

/**
 * The 继续阅读 banner appears for a real mid-article position: Karakeep's
 * 10–99% window, so a first glance does not nag and a finished read does not
 * offer to resume.
 */
export function shouldOfferResume(article: Pick<ReadingArticle, 'progress_percent' | 'read_at'>): boolean {
  return article.read_at === null && article.progress_percent >= 10 && article.progress_percent < 100;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

/** Accepts what a user pastes: a bare host is promoted to https. */
export function normalizeInputUrl(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const candidate = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  try {
    const url = new URL(candidate);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
    if (!url.hostname.includes('.') && url.hostname !== 'localhost') return null;
    return url.toString();
  } catch {
    return null;
  }
}

/**
 * Rewrites the stored `images/<file>` references to URLs the webview may
 * load. In the Tauri shell that is the asset protocol scoped to
 * `~/.todo/reading`; elsewhere the paths are left alone (mock content has none).
 */
export function resolveImageSources(contentHtml: string, imageDir: string): string {
  if (!isTauriRuntime()) return contentHtml;
  return contentHtml.replace(/src="images\/([^"]+)"/g, (_match, file: string) => {
    const path = `${imageDir}/${file}`;
    return `src="${convertFileSrc(path)}"`;
  });
}

export function coverSource(article: ReadingArticle): string | null {
  if (!article.cover_path) return null;
  if (!isTauriRuntime()) return article.cover_path.startsWith('http') ? article.cover_path : null;
  return convertFileSrc(article.cover_path);
}

// ── Anchors ──────────────────────────────────────────────────────────────────

/**
 * The anchor is the index of the first block whose bottom edge is below the
 * top of the scroll viewport: what the reader sees first. Blocks are numbered
 * in document order over the stored body, which never changes, so an index
 * points to the same paragraph next week.
 */
export const ANCHOR_ATTRIBUTE = 'data-r-idx';
const BLOCK_SELECTOR = 'p, h1, h2, h3, h4, h5, h6, li, blockquote, pre, figure, table, img, section > div';

export function numberBlocks(root: HTMLElement): HTMLElement[] {
  const blocks = Array.from(root.querySelectorAll<HTMLElement>(BLOCK_SELECTOR)).filter(
    (element) => !element.querySelector(BLOCK_SELECTOR) || element.tagName === 'LI' || element.tagName === 'FIGURE',
  );
  blocks.forEach((element, index) => element.setAttribute(ANCHOR_ATTRIBUTE, String(index)));
  return blocks;
}

export function anchorFromScroll(blocks: HTMLElement[], scroller: HTMLElement): string | null {
  const top = scroller.getBoundingClientRect().top;
  for (const block of blocks) {
    const rect = block.getBoundingClientRect();
    if (rect.bottom > top + 4) return `idx:${block.getAttribute(ANCHOR_ATTRIBUTE)}`;
  }
  return null;
}

export function parseAnchor(anchor: string | null | undefined): number | null {
  if (!anchor) return null;
  const match = /^idx:(\d+)$/.exec(anchor);
  return match ? Number(match[1]) : null;
}

/** Percentage read: how far the bottom of the viewport is into the content. */
export function percentFromScroll(scroller: HTMLElement): number {
  const total = scroller.scrollHeight;
  const visible = scroller.clientHeight;
  if (total <= visible + 2) return 100;
  const seen = scroller.scrollTop + visible;
  return Math.max(0, Math.min(100, Math.round((seen / total) * 100)));
}

// ── Transport ────────────────────────────────────────────────────────────────

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export class ReadingError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ReadingError';
  }
}

function asReadingError(value: unknown): ReadingError {
  if (value instanceof ReadingError) return value;
  if (value instanceof Error) return new ReadingError(value.message);
  if (typeof value === 'string') return new ReadingError(value);
  return new ReadingError('阅读库返回了未知错误');
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asReadingError(error);
  }
}

// Mock library for the browser build: two articles so the list, the reader,
// the resume banner and annotations all have something to show.
const MOCK_BODY = `
<p>在信息过载的时代，我们每天都在接收大量的知识碎片。真正重要的，不是你读了多少，而是你是否有一套可持续的方法，将这些信息转化为自己的认知资产。</p>
<h2>一、从收集到结构化</h2>
<p>很多人把知识管理等同于「收藏」。但收藏只是开始，结构化才是关键。你需要为不同类型的内容找到合适的位置，让它们能够在未来被重新发现、重新组合。</p>
<p>知识的价值不在于拥有，而在于能够被激活和使用。一个可持续的知识系统，应该像花园一样，既有秩序，又能自然生长。</p>
<p>结构化的方法有很多，比如卡片笔记、PARA 方法、双向链接等。重要的是找到适合自己的方式，并坚持下去。不要追求完美的体系，先让它运转起来，再在实践中不断迭代。</p>
<h2>二、让知识服务于生活</h2>
<p>知识系统的最终目的是更好地生活，而不是成为另一个收藏的黑洞。你可以定期回顾自己的笔记，尝试将其中的想法应用到工作、学习或生活中。</p>
<p>当知识开始影响你的选择、改变你的行为时，它才真正成为了你的一部分。这也是持续建设知识系统的最大动力。</p>
<h2>三、小步快跑</h2>
<p>每天记录一条想法，每周整理一次，每月回顾一次。节奏比强度重要。</p>
<p>阅读的意义，不在于记住多少内容，而在于它能否改变你看待世界的方式。</p>
`;

const mockArticles: ReadingArticleDetail[] = [
  {
    id: 'rmock1',
    url: 'https://mp.weixin.qq.com/s/mock-knowledge-system',
    title: '如何建立一个可持续的个人知识系统',
    site_name: '长期主义者',
    excerpt: '不是收藏更多，而是让知识真正为你所用。构建属于自己的知识系统，是长期主义的起点。',
    byline: '长期主义者',
    cover_path: null,
    word_count: 420,
    reading_minutes: 2,
    added_at: Date.now() - 3 * 86_400_000,
    last_opened_at: Date.now() - 86_400_000,
    progress_percent: 42,
    progress_anchor: 'idx:4',
    read_at: null,
    annotation_count: 1,
    content_html: MOCK_BODY,
    markdown: null,
    image_dir: '',
    annotations: [
      {
        id: 'amock1',
        article_id: 'rmock1',
        quote: '知识的价值不在于拥有，而在于能够被激活和使用。',
        start: 0,
        end: 0,
        note: '非常认同这句话。知识不是囤积，而是流动起来。',
        created_at: Date.now() - 86_400_000,
        updated_at: Date.now() - 86_400_000,
      },
    ],
  },
  {
    id: 'rmock2',
    url: 'https://example.com/deep-work',
    title: '深度工作：在注意力稀缺的时代，如何掌控自己',
    site_name: 'example.com',
    excerpt: '深度工作不是一种技巧，而是一种生活方式。',
    byline: null,
    cover_path: null,
    word_count: 380,
    reading_minutes: 1,
    added_at: Date.now() - 7 * 86_400_000,
    last_opened_at: null,
    progress_percent: 0,
    progress_anchor: null,
    read_at: null,
    annotation_count: 0,
    content_html: MOCK_BODY,
    markdown: null,
    image_dir: '',
    annotations: [],
  },
];

// Mock annotation offsets are resolved against the rendered text on first
// load, since they are stored as the quote only.
function mockSummary(detail: ReadingArticleDetail): ReadingArticle {
  const { content_html: _html, markdown: _md, image_dir: _dir, annotations, ...rest } = detail;
  return { ...rest, annotation_count: annotations.length };
}

function mockFind(id: string): ReadingArticleDetail {
  const found = mockArticles.find((article) => article.id === id);
  if (!found) throw new ReadingError(`阅读记录不存在: ${id}`);
  return found;
}

let mockSequence = 3;

export const reading = {
  /** Whether this build can hold a library at all. */
  available(): boolean {
    return isTauriRuntime() || !(typeof window !== 'undefined' && window.location.protocol.startsWith('http') && !window.location.hostname.includes('localhost'));
  },

  async list(): Promise<ReadingArticle[]> {
    if (isTauriRuntime()) return call<ReadingArticle[]>('reading_list');
    return mockArticles.map(mockSummary);
  },

  async importBrowserPage(url: string, html: string): Promise<ReadingArticleDetail> {
    if (isTauriRuntime()) return call<ReadingArticleDetail>('reading_import_browser', { url, html });
    return this.add(url);
  },

  async add(url: string): Promise<ReadingArticleDetail> {
    if (isTauriRuntime()) return call<ReadingArticleDetail>('reading_add', { url });
    await new Promise((resolve) => window.setTimeout(resolve, 600));
    const existing = mockArticles.find((article) => article.url === url);
    if (existing) return existing;
    const detail: ReadingArticleDetail = {
      id: `rmock${mockSequence++}`,
      url,
      title: `示例文章 · ${hostOf(url)}`,
      site_name: hostOf(url),
      excerpt: '浏览器模式没有抓取能力，这是一篇占位文章。',
      byline: null,
      cover_path: null,
      word_count: 420,
      reading_minutes: 2,
      added_at: Date.now(),
      last_opened_at: null,
      progress_percent: 0,
      progress_anchor: null,
      read_at: null,
      annotation_count: 0,
      content_html: MOCK_BODY,
      markdown: null,
      image_dir: '',
      annotations: [],
    };
    mockArticles.unshift(detail);
    return detail;
  },

  async get(id: string): Promise<ReadingArticleDetail> {
    if (isTauriRuntime()) return call<ReadingArticleDetail>('reading_get', { id });
    const found = mockFind(id);
    found.last_opened_at = Date.now();
    return { ...found, annotations: found.annotations.map((annotation) => ({ ...annotation })) };
  },

  async setProgress(id: string, percent: number, anchor: string | null): Promise<ReadingArticle> {
    if (isTauriRuntime()) return call<ReadingArticle>('reading_progress', { id, percent, anchor });
    const found = mockFind(id);
    if (found.read_at !== null && found.progress_percent === 100) return mockSummary(found);
    found.progress_percent = Math.min(100, percent);
    found.progress_anchor = percent === 0 || percent >= 100 ? null : anchor;
    found.read_at = percent >= 100 ? Date.now() : null;
    return mockSummary(found);
  },

  async setRead(id: string, read: boolean): Promise<ReadingArticle> {
    if (isTauriRuntime()) return call<ReadingArticle>('reading_set_read', { id, read });
    const found = mockFind(id);
    if (read) {
      found.progress_percent = 100;
      found.progress_anchor = null;
      found.read_at = Date.now();
    } else {
      found.read_at = null;
      if (found.progress_percent === 100) found.progress_percent = 0;
    }
    return mockSummary(found);
  },

  async remove(id: string): Promise<void> {
    if (isTauriRuntime()) return call<void>('reading_remove', { id });
    const index = mockArticles.findIndex((article) => article.id === id);
    if (index >= 0) mockArticles.splice(index, 1);
  },

  async upsertAnnotation(annotation: AnnotationInput): Promise<ReadingAnnotation> {
    if (isTauriRuntime()) return call<ReadingAnnotation>('reading_annotation_upsert', { annotation });
    const found = mockFind(annotation.article_id);
    const now = Date.now();
    const existing = found.annotations.find((item) => item.id === annotation.id);
    if (existing) {
      Object.assign(existing, { quote: annotation.quote, start: annotation.start, end: annotation.end, note: annotation.note, updated_at: now });
      return { ...existing };
    }
    const created: ReadingAnnotation = { ...annotation, created_at: now, updated_at: now };
    found.annotations.push(created);
    return { ...created };
  },

  async deleteAnnotation(articleId: string, id: string): Promise<void> {
    if (isTauriRuntime()) return call<void>('reading_annotation_delete', { id });
    const found = mockFind(articleId);
    found.annotations = found.annotations.filter((item) => item.id !== id);
  },
};
