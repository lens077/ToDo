/**
 * The AI feed: agent-session records published by an external host
 * (DeepSeek Harness first) through `todo ai upsert`. Contract:
 * `docs/ai-feed-spec.md`.
 *
 * Records are not Project Tasks. The host owns `state`; ToDo only adds local
 * marks (`acked_at`, `hidden`, `task_id`). Everything here is pure so the view
 * and the sidebar badge derive the same numbers from the same list.
 */

export type AiState = 'running' | 'waiting' | 'unread' | 'seen' | 'failed' | 'handled';
export type AiOutcome = 'completed' | 'error' | 'aborted' | 'blocked' | 'max-tokens' | 'interrupted';

export interface AiRecord {
  /** Optional source revision from the DSH bridge. */
  ai_feed_source_revision?: string;
  source: string;
  session_id: string;
  workspace: string;
  state: AiState;
  outcome: AiOutcome | null;
  title: string | null;
  question: string | null;
  reply: string | null;
  reply_truncated: boolean;
  changed_files: string[];
  changed_file_count: number;
  open_url: string | null;
  /** Host-side event time, epoch milliseconds. */
  event_at: number;
  received_at: number;
  acked_at: number | null;
  hidden: boolean;
  task_id: string | null;
}

export interface AiListResult {
  records: AiRecord[];
  /** A host has published at least once: the store file exists. */
  connected: boolean;
}

export type AiScope = 'project' | 'all';

/** Contract order: what needs the user first. */
export const AI_SECTION_ORDER: AiState[] = ['waiting', 'failed', 'unread', 'running', 'seen', 'handled'];

/** Pill order from the UI spec; the sections keep the contract order. */
export const AI_PILL_ORDER: AiState[] = ['running', 'waiting', 'unread', 'failed', 'seen', 'handled'];

export const AI_STATE_LABEL: Record<AiState, string> = {
  running: '运行中',
  waiting: '等你回复',
  unread: '完成未读',
  failed: '失败',
  seen: '已读',
  handled: '已处理',
};

export const AI_OUTCOME_LABEL: Record<AiOutcome, string> = {
  completed: '完成',
  error: '出错',
  aborted: '中止',
  blocked: '受阻',
  'max-tokens': '超出上下文',
  interrupted: '被打断',
};

/** States that count toward the sidebar badge before any local mark. */
const ATTENTION: ReadonlySet<AiState> = new Set<AiState>(['waiting', 'failed', 'unread']);

/**
 * Where a record is shown. `state` belongs to the host and never changes here;
 * a local ack moves an attention-worthy record into 已读 until the host
 * publishes newer activity (which clears `acked_at`, see spec §3.2).
 */
export function displayState(record: AiRecord): AiState {
  if (record.acked_at !== null && ATTENTION.has(record.state)) return 'seen';
  return record.state;
}

export function canAck(record: AiRecord): boolean {
  return record.acked_at === null && ATTENTION.has(record.state);
}

export function needsAttention(record: AiRecord): boolean {
  return ATTENTION.has(displayState(record));
}

/**
 * Whether `workspace` is `root` or a directory under it. Path-segment aware:
 * `/a/b` is not under `/a/bc`.
 */
export function inWorkspace(workspace: string, root: string): boolean {
  const normalize = (path: string) => path.replace(/\\/g, '/').replace(/\/+$/, '');
  const target = normalize(workspace);
  const base = normalize(root);
  if (!base) return false;
  return target === base || target.startsWith(`${base}/`);
}

/** The project root a snapshot describes: its directory, never `TODO.md`. */
export function projectRoot(projectPath: string | null | undefined): string | null {
  if (!projectPath) return null;
  const normalized = projectPath.replace(/\\/g, '/');
  return /\/todo\.md$/i.test(normalized) ? normalized.replace(/\/[^/]*$/, '') : normalized.replace(/\/+$/, '');
}

/** Visible records for a scope: hidden ones are never shown. */
export function scopedRecords(records: AiRecord[], scope: AiScope, root: string | null): AiRecord[] {
  return records.filter((record) => {
    if (record.hidden) return false;
    if (scope === 'all' || root === null) return true;
    return inWorkspace(record.workspace, root);
  });
}

export function attentionCount(records: AiRecord[]): number {
  return records.filter(needsAttention).length;
}

export function countByState(records: AiRecord[]): Record<AiState, number> {
  const counts: Record<AiState, number> = { running: 0, waiting: 0, unread: 0, seen: 0, failed: 0, handled: 0 };
  for (const record of records) counts[displayState(record)] += 1;
  return counts;
}

export interface AiSection {
  state: AiState;
  records: AiRecord[];
}

/** Sections in contract order; empty sections are omitted. */
export function groupSections(records: AiRecord[], visible: ReadonlySet<AiState>): AiSection[] {
  const byState = new Map<AiState, AiRecord[]>();
  for (const record of records) {
    const state = displayState(record);
    if (!visible.has(state)) continue;
    const bucket = byState.get(state) ?? [];
    bucket.push(record);
    byState.set(state, bucket);
  }
  return AI_SECTION_ORDER.flatMap((state) => {
    const bucket = byState.get(state);
    if (!bucket?.length) return [];
    return [{ state, records: [...bucket].sort((a, b) => b.event_at - a.event_at) }];
  });
}

export function workspaceName(workspace: string): string {
  return workspace.replace(/\\/g, '/').split('/').filter(Boolean).at(-1) ?? workspace;
}

/**
 * How a changed file is labelled on a card. The contract asks for paths
 * relative to `workspace`, but hosts do send absolute ones: those are shown
 * relative to the workspace when they live under it, and by basename
 * otherwise, with the full path kept for the tooltip.
 */
export function displayPath(file: string, workspace: string): { label: string; title: string } {
  const normalized = file.replace(/\\/g, '/');
  const root = workspace.replace(/\\/g, '/').replace(/\/+$/, '');
  if (root && normalized.startsWith(`${root}/`) && normalized.length > root.length + 1) {
    return { label: normalized.slice(root.length + 1), title: file };
  }
  const absolute = normalized.startsWith('/') || /^[a-zA-Z]:\//.test(normalized);
  if (absolute) {
    const base = normalized.split('/').filter(Boolean).at(-1);
    return { label: base ?? file, title: file };
  }
  return { label: file, title: file };
}

/** "3 分钟前"-style time, without exclamation or precision it cannot have. */
export function relativeTime(eventAt: number, now = Date.now()): string {
  const delta = Math.max(0, now - eventAt);
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (delta < minute) return '刚刚';
  if (delta < hour) return `${Math.floor(delta / minute)} 分钟前`;
  if (delta < day) return `${Math.floor(delta / hour)} 小时前`;
  if (delta < 2 * day) return '昨天';
  if (delta < 30 * day) return `${Math.floor(delta / day)} 天前`;
  const date = new Date(eventAt);
  return Number.isNaN(date.getTime())
    ? ''
    : new Intl.DateTimeFormat('zh-CN', { month: 'short', day: 'numeric' }).format(date);
}

export function absoluteTime(eventAt: number): string {
  const date = new Date(eventAt);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleString('zh-CN', { hour12: false });
}

export function recordKey(record: Pick<AiRecord, 'source' | 'session_id'>): string {
  return `${record.source}/${record.session_id}`;
}
