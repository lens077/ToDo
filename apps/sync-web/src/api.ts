/// virgo-sync HTTP API 客户端。类型与 `todo-core` 的 reading 序列化对齐。

export interface SyncConfig {
  base: string;
  token: string;
}

export interface ReadingArticle {
  id: string;
  url: string;
  title: string;
  site_name: string;
  excerpt: string;
  byline: string | null;
  word_count: number;
  reading_minutes: number;
  added_at: number;
  last_opened_at: number | null;
  progress_percent: number;
  read_at: number | null;
}

export interface ReadingContext {
  saved_reason: string;
  focus_questions: string;
  revisit_condition: string;
  personal_notes: string;
  revisit_type: string;
  parse_status: string;
  project_links: unknown[];
  task_links: unknown[];
}

export interface ReadingArticleDetail extends ReadingArticle {
  content_html: string;
  markdown: string | null;
  annotations: Array<{ id: string; quote: string; note: string }>;
  context: ReadingContext;
}

function headers(config: SyncConfig): HeadersInit {
  return { Authorization: `Bearer ${config.token}` };
}

async function request<T>(config: SyncConfig, path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${config.base}${path}`, {
    ...init,
    headers: { ...headers(config), ...(init?.headers ?? {}) },
  });
  if (!response.ok) {
    const body = await response.text().catch(() => '');
    throw new Error(`${response.status} ${body}`.trim());
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export function listArticles(config: SyncConfig): Promise<{ articles: ReadingArticle[] }> {
  return request(config, '/articles');
}

export function getArticle(config: SyncConfig, id: string): Promise<ReadingArticleDetail> {
  return request(config, `/articles/${id}`);
}

export function saveLink(
  config: SyncConfig,
  url: string,
  notes?: string,
): Promise<ReadingArticleDetail> {
  return request(config, '/links', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url, context: notes ? { personal_notes: notes } : undefined }),
  });
}

export function updateNotes(
  config: SyncConfig,
  id: string,
  personalNotes: string,
): Promise<ReadingArticleDetail> {
  return request(config, `/articles/${id}/context`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ personal_notes: personalNotes }),
  });
}

export function setRead(config: SyncConfig, id: string, read: boolean): Promise<unknown> {
  return request(config, `/articles/${id}/read`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ read }),
  });
}

export function removeArticle(config: SyncConfig, id: string): Promise<void> {
  return request(config, `/articles/${id}`, { method: 'DELETE' });
}

export function refetch(config: SyncConfig, id: string): Promise<unknown> {
  return request(config, `/articles/${id}/refetch`, { method: 'POST' });
}

/**
 * Subscribes to the server's SSE change stream. EventSource cannot send an
 * Authorization header, so the stream is parsed from a fetch body instead.
 * Returns a cleanup function.
 */
export function subscribeChanges(config: SyncConfig, onChange: () => void): () => void {
  const controller = new AbortController();
  let closed = false;
  void (async () => {
    while (!closed) {
      try {
        const response = await fetch(`${config.base}/stream`, {
          headers: headers(config),
          signal: controller.signal,
        });
        if (!response.ok || !response.body) throw new Error(String(response.status));
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          // SSE 事件以空行分隔；每个 `event: change` 都代表一次变更。
          while (buffer.includes('\n\n')) {
            const chunk = buffer.slice(0, buffer.indexOf('\n\n') + 2);
            buffer = buffer.slice(buffer.indexOf('\n\n') + 2);
            if (chunk.includes('event: change')) onChange();
          }
        }
      } catch {
        if (closed) return;
      }
      if (!closed) await new Promise((resolve) => setTimeout(resolve, 3000));
    }
  })();
  return () => {
    closed = true;
    controller.abort();
  };
}
