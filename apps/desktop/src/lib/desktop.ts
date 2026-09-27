import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { sourceKey, sourceQuery } from './taskSources';
import type { AiListResult, AiOutcome, AiRecord, AiState } from './ai';
import type { OpenSessionMode } from './appSettings';
import { readColumnSetupOptions, readColumnSetupPreview, type ColumnSetupOptions, type ColumnSetupPreview, type ColumnSetupSelection } from './columnSetup';
export type { ColumnSetupOptions, ColumnSetupPreview, ColumnSetupSelection } from './columnSetup';
import {
  api,
  HttpError,
  apiBase,
  detectTransport,
  probeHttp,
  type TransportCapabilities,
  type TransportKind,
} from './transport';

export type TaskPriority = 1 | 2 | 3 | 4 | null;
export type PriorityScope = 'task' | 'subtree';

export interface DesktopColumn {
  source_path?: string;
  local_id?: string;
  id: string;
  name: string;
  done: boolean;
}

export type CompletionPolicy = 'normal' | 'strict';

export interface EvidenceRange {
  from: string;
  to: string;
}

export type EvidenceAvailability =
  | ({ status: 'available' } & EvidenceRange)
  | { status: 'unavailable'; reason: string };

export interface DesktopTask {
  source_path?: string;
  local_id?: string | null;
  source_writable?: boolean;
  id: string | null;
  body: string;
  completed: boolean;
  column: string | null;
  indent_level: number;
  parent_id: string | null;
  due: string | null;
  priority: TaskPriority;
  tags: string[];
  /** Partial completion percentage (1-99) when the task carries a `◐` token. */
  progress: number | null;
  /** Date of the last failed attempt when the task carries a `⚠` token. */
  failed_on: string | null;
  /** Text of the `↳` note line. */
  note: string | null;
  /** Persisted `from..to` evidence text, when present. */
  evidence: string | null;
  /** Git range offered when confirming strict completion. */
  evidence_availability: EvidenceAvailability;
}

export type DiffKind = 'context' | 'added' | 'removed';

export interface DiffHunk {
  left_line: number | null;
  right_line: number | null;
  kind: DiffKind;
  text: string;
}

export interface TextDiff {
  hunks: DiffHunk[];
  added: number;
  removed: number;
}

/**
 * A whole-file rewrite awaiting confirmation.
 *
 * `base_hash` pins the document the diff was computed against; applying it after
 * the file moved on is refused, so a user never confirms one diff and gets another.
 */
export interface ProposedRewrite {
  diff: TextDiff;
  proposed: string;
  base_hash: string;
}

export interface LooseCheckbox {
  path: string;
  line: number;
  text: string;
  completed: boolean;
}

export interface CodeComment {
  path: string;
  line: number;
  marker: string;
  text: string;
}

export interface ScanReport {
  loose_checkboxes: LooseCheckbox[];
  code_comments: CodeComment[];
  stats: {
    files_scanned: number;
    files_skipped: number;
    duplicates_filtered: number;
  };
}

export interface ScanPreview {
  report: ScanReport;
  rewrite: ProposedRewrite | null;
}

export interface HistoryEntry {
  version: number;
  at: string;
  file: string;
  op: string;
  bytes: number;
  blob: string;
}

export interface TodayView {
  overdue: DesktopTask[];
  today: DesktopTask[];
  upcoming: DesktopTask[];
  /** Actionable work with no due date, ordered by priority. */
  next: DesktopTask[];
}

export interface TaskSource {
  completion_policy?: CompletionPolicy;
  path: string;
  status: 'managed' | 'unmanaged' | 'error';
  writable: boolean;
  preview: string;
  preview_truncated?: boolean;
  /** Bullet lines the loose list reader (Beta) would treat as tasks. */
  loose_candidates?: number;
  /** This snapshot read the source with the loose reader; it is read-only. */
  loose?: boolean;
  error?: string | null;
  revision: string;
  task_count: number;
}

export interface AdoptPreview extends ProposedRewrite {
  source_path: string;
}

export interface DesktopSnapshot {
  sources?: TaskSource[];
  selected_source?: string | null;
  warnings?: string[];
  project_path: string;
  revision: string;
  completion_policy: CompletionPolicy;
  columns: DesktopColumn[];
  tasks: DesktopTask[];
  today: TodayView;
}

export interface Capture {
  id: string;
  created_at: string;
  text: string;
}

export type ShortcutStatus =
  | { status: 'pending' }
  | { status: 'registered' }
  | { status: 'error'; message: string };

export interface DesktopStatus {
  startup_ms: number | null;
  shortcut_status: ShortcutStatus;
}

export interface CaptureAppendResult {
  capture: Capture;
  captures: Capture[];
}

export type MoveTarget =
  | { type: 'column_start' }
  | { type: 'column_end' }
  | { type: 'before'; task_id: string }
  | { type: 'after'; task_id: string };

export interface MoveCompletionDetails {
  note?: string;
  evidence?: EvidenceRange;
}

export type DesktopMutation =
  | { type: 'add_task'; body: string; column: string | null }
  | { type: 'set_completed'; id: string; completed: boolean; note?: string; evidence?: EvidenceRange }
  | { type: 'set_partial'; id: string; percent: number; note: string }
  | { type: 'set_failed'; id: string; note: string }
  | { type: 'set_body'; id: string; body: string }
  | { type: 'set_priority'; id: string; priority: TaskPriority; scope?: PriorityScope }
  | ({ type: 'move_task'; id: string; column: string; target: MoveTarget } & MoveCompletionDetails);

type TriageOutcome =
  | { status: 'completed'; task_id: string }
  | { status: 'task_created_capture_retained'; task_id: string; message: string };

export type TriageResult = TriageOutcome & {
  snapshot?: DesktopSnapshot;
  captures?: Capture[];
  snapshot_error?: string;
  captures_error?: string;
};

export type DesktopEvent =
  | { type: 'snapshot'; snapshot: DesktopSnapshot }
  | { type: 'error'; error: DesktopBridgeError };

/** What 转为任务 returns: the new task, the linked record, and the board if it could be re-read. */
export interface AiToTaskResult {
  task_id: string;
  record: AiRecord;
  snapshot?: DesktopSnapshot;
  snapshot_error?: string;
}

export class DesktopBridgeError extends Error {
  readonly kind: 'conflict' | 'locked' | 'parse' | 'no_project' | 'unknown';

  constructor(message: string, kind = classifyError(message)) {
    super(message);
    this.name = 'DesktopBridgeError';
    this.kind = kind;
  }
}

/** The browser tab 「复用同一窗口」 targets on the web: one name, one tab. */
const SESSION_TAB_NAME = 'todo-dsh';

declare global {
  interface Window {
    /** Mock mode only: every 打开会话 call, for e2e to inspect. */
    __todoMockOpenSessions?: Array<{ url: string; mode: OpenSessionMode }>;
  }
}

const MOCK_TODAY = '2026-08-31';
type MockMode = 'default' | 'empty' | 'no-project' | 'load-error';
let mockMode: MockMode = (() => {
  if (typeof window === 'undefined') return 'default';
  const requested = new URLSearchParams(window.location.search).get('mock');
  return requested === 'empty' || requested === 'no-project' || requested === 'load-error' ? requested : 'default';
})();
const mockColumns: DesktopColumn[] = [
  { id: 'inbox', name: '待梳理', done: false },
  { id: 'doing', name: '正在推进', done: false },
  { id: 'review', name: '等待验证', done: false },
  { id: 'done', name: '已完成', done: true },
];

const mockInitialTasks: DesktopTask[] = [
  {
    id: 'fl8c2a',
    body: '为 reconcile findings 补齐 evidence diff 摘要',
    completed: false,
    column: 'doing',
      indent_level: 0,
      parent_id: null,
    due: '2026-08-30',
    priority: 1,
    tags: ['core', 'evidence'],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'available', from: 'abc1234', to: 'def5678' },
  },
  {
    id: 'k9p4dr',
    body: '测量 1,000 项 Snapshot 构建预算',
    completed: false,
    column: 'doing',
      indent_level: 0,
      parent_id: null,
    due: MOCK_TODAY,
    priority: 2,
    tags: ['desktop'],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
  },
  {
    id: 'm2v7qx',
    body: '完善 agent hook 安装后的首次使用说明',
    completed: false,
    column: 'inbox',
      indent_level: 0,
      parent_id: null,
    due: '2026-09-02',
    priority: 2,
    tags: ['agents', 'docs'],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
  },
  {
    id: 'a4n8sj',
    body: '补充文件冲突恢复的端到端覆盖',
    completed: false,
    column: 'review',
      indent_level: 0,
      parent_id: null,
    due: null,
    priority: 1,
    tags: ['rust'],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
  },
  {
    id: 'w6h0bp',
    body: '发布前跑一遍键盘与屏幕阅读器路径',
    completed: false,
    column: 'inbox',
      indent_level: 0,
      parent_id: null,
    due: '2026-09-06',
    priority: 3,
    tags: ['qa'],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
  },
  {
    id: 'd8q3mf',
    body: '确定 TODO.md v1 的保真写回边界',
    completed: true,
    column: 'done',
      indent_level: 0,
      parent_id: null,
    due: '2026-08-28',
    priority: 2,
    tags: ['format'],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
  },
];

let mockRevision = 17;
let mockTasks = cloneTasks(mockInitialTasks);
let mockCaptures: Capture[] = [
  {
    id: 'cap-b7k2',
    created_at: '2026-08-31T08:42:00Z',
    text: '把外部编辑冲突 banner 的复现步骤补到验收记录里',
  },
  {
    id: 'cap-n4w8',
    created_at: '2026-08-31T09:17:00Z',
    text: '研究 Windows 全局快捷键是否与系统输入法冲突',
  },
  {
    id: 'cap-r2m6',
    created_at: '2026-08-30T16:05:00Z',
    text: '把 README 的首次使用路径缩短到三步',
  },
];
let mockCaptureSequence = 4;
let mockTaskSequence = 1;
const mockListeners = new Set<(event: DesktopEvent) => void>();
const mockAiListeners = new Set<() => void>();

// `?ai=none` is a machine where no host has published yet; `?ai=empty` is a
// connected host with nothing for any workspace. The default covers every
// state and two workspaces so the scope toggle and the sections have work.
type MockAiMode = 'default' | 'none' | 'empty';
const mockAiMode: MockAiMode = (() => {
  if (typeof window === 'undefined') return 'default';
  const requested = new URLSearchParams(window.location.search).get('ai');
  return requested === 'none' || requested === 'empty' ? requested : 'default';
})();
const MOCK_AI_ROOT = '/Users/demo/field-notes';
const mockAiNow = Date.now();
/** 300 characters with no break opportunity, the worst case for a card's width. */
const MOCK_AI_LONG_TOKEN = 'gateway-manifest-'.concat('x'.repeat(283));
/** 32 absolute paths under the record's workspace, as a host that ignores §1.1 sends them. */
const MOCK_AI_WIDE_FILES: string[] = Array.from({ length: 32 }, (_, index) =>
  index === 0
    ? `${MOCK_AI_ROOT}/kubernetes/components/gateway/README.md`
    : `${MOCK_AI_ROOT}/kubernetes/components/gateway/manifests/environment-${String(index).padStart(2, '0')}/deployment-with-a-deliberately-long-name.yaml`);
function mockAiRecord(
  overrides: Partial<AiRecord> & Pick<AiRecord, 'session_id' | 'state' | 'event_at'>,
): AiRecord {
  return {
    source: 'dsh',
    workspace: MOCK_AI_ROOT,
    outcome: null,
    title: null,
    question: null,
    reply: null,
    reply_truncated: false,
    changed_files: [],
    changed_file_count: 0,
    open_url: `http://127.0.0.1:3080/?session=${overrides.session_id}`,
    received_at: overrides.event_at,
    acked_at: null,
    hidden: false,
    task_id: null,
    ...overrides,
  };
}
let mockAiRecords: AiRecord[] = mockAiMode === 'default' ? [
  mockAiRecord({
    session_id: 's_wait1',
    state: 'waiting',
    event_at: mockAiNow - 4 * 60_000,
    title: '为 CLI 增加 release 构建说明',
    question: '要不要把 cargo install --path 的说明也写进 README 的快速开始？',
    reply: '我准备在「快速开始」第二段加入 cargo install 的命令，并在下面注明需要 Rust 1.80 以上。是否继续？',
  }),
  mockAiRecord({
    session_id: 's_fail1',
    state: 'failed',
    outcome: 'error',
    event_at: mockAiNow - 26 * 60_000,
    title: '修复 watcher 的读取反馈循环',
    question: '把 Linux 上的 Access(Open) 事件过滤掉',
    reply: 'cargo test 在 watch::tests::reload_callback_reads_do_not_create_a_feedback_loop 失败：second 仍收到 Reload。已回退改动。',
    changed_files: ['apps/desktop/src-tauri/src/watch.rs'],
    changed_file_count: 1,
  }),
  mockAiRecord({
    session_id: 's_unread1',
    state: 'unread',
    outcome: 'completed',
    workspace: `${MOCK_AI_ROOT}/apps/desktop`,
    event_at: mockAiNow - 3 * 3_600_000,
    title: '补齐 evidence diff 摘要',
    question: '为 reconcile findings 补齐 evidence diff 摘要，并加上单元测试',
    reply: '已在 reconcile.rs 增加 evidence_summary()，输出「+N/-M 行」与涉及文件数；对应测试 3 个全部通过。\n\n顺带修正了 diff_lines 在空文件上的越界。README 的对账章节补了一句示例输出。',
    changed_files: ['crates/todo-core/src/reconcile.rs', 'crates/todo-core/src/write.rs', 'crates/todo-core/tests/reconcile.rs', 'README.md', 'docs/spec.md'],
    changed_file_count: 7,
  }),
  // No link yet: the host publishes `open_url` once the turn has an id to
  // deep-link to, so 打开会话 is disabled here.
  mockAiRecord({
    session_id: 's_run1',
    state: 'running',
    event_at: mockAiNow - 30_000,
    question: '把 1,000 项 Snapshot 的构建预算测一遍，输出到 docs/perf.md',
    open_url: null,
  }),
  mockAiRecord({
    session_id: 's_other1',
    state: 'unread',
    outcome: 'completed',
    workspace: '/Users/demo/site-notes',
    event_at: mockAiNow - 40 * 60_000,
    title: '把博客构建脚本迁到 pnpm',
    question: '把 Makefile 的 build 目标迁到 pnpm scripts',
    reply: '已迁移；package.json 新增 build/preview 两个脚本，Makefile 保留为转发。',
    changed_files: ['package.json', 'Makefile'],
    changed_file_count: 2,
  }),
  // A host that sent absolute paths, the full 32-file list, and an unbroken
  // token: the card must still fit its column (e2e asserts the geometry).
  // Already seen, so the attention counts the other tests rely on hold.
  mockAiRecord({
    session_id: 's_wide1',
    state: 'seen',
    outcome: 'completed',
    workspace: '/Users/demo/field-notes/kubernetes',
    event_at: mockAiNow - 55 * 60_000,
    title: '整理 gateway 组件的 README 与部署清单',
    question: `把 ${MOCK_AI_LONG_TOKEN} 这个 manifest 的名字改短，并把 kubernetes/components/gateway 下所有 README 的安装步骤统一成同一份`,
    reply: `已把 32 个文件的安装步骤改为引用 components/gateway/README.md。原来的 ${MOCK_AI_LONG_TOKEN} 改为 gateway-manifest.yaml。`,
    changed_files: MOCK_AI_WIDE_FILES,
    changed_file_count: MOCK_AI_WIDE_FILES.length,
  }),
  mockAiRecord({
    session_id: 's_seen1',
    state: 'seen',
    outcome: 'completed',
    event_at: mockAiNow - 2 * 86_400_000,
    title: '整理 e2e 夹具',
    reply: '夹具已收敛到 desktop.ts 一处。',
  }),
  mockAiRecord({
    session_id: 's_handled1',
    state: 'handled',
    outcome: 'completed',
    event_at: mockAiNow - 5 * 86_400_000,
    title: '确定 TODO.md v1 的保真写回边界',
    reply: '边界已写入 docs/spec.md §4。',
    task_id: 'd8q3mf',
  }),
] : [];

function emitMockAi(): void {
  window.setTimeout(() => {
    mockAiListeners.forEach((listener) => listener());
  }, 24);
}

function mockAiModify(source: string, session: string, change: (record: AiRecord) => AiRecord): AiRecord {
  const index = mockAiRecords.findIndex((record) => record.source === source && record.session_id === session);
  if (index === -1) throw new DesktopBridgeError(`会话 ${source}/${session} 已不存在`);
  const next = change(mockAiRecords[index]);
  mockAiRecords = mockAiRecords.map((record, position) => (position === index ? next : record));
  emitMockAi();
  return { ...next, changed_files: [...next.changed_files] };
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Whether a `todo serve` backend answered, and what it allows.
 *
 * Resolved once at startup by `initTransport`. Until then the bridge behaves as
 * it always has, so nothing regresses if the probe is slow or never runs.
 */
let httpCapabilities: TransportCapabilities | null = null;

function useHttp(): boolean {
  return !isTauriRuntime() && httpCapabilities !== null;
}

/**
 * Detects the backend once, before the first render that depends on it.
 *
 * Returns the transport actually in use, so the shell can tell the user it is
 * read-only rather than letting writes fail one by one.
 */
export async function initTransport(): Promise<{
  kind: TransportKind;
  capabilities: TransportCapabilities | null;
}> {
  if (isTauriRuntime()) return { kind: 'tauri', capabilities: null };
  if (detectTransport() === 'http') {
    httpCapabilities = await probeHttp();
    if (httpCapabilities) return { kind: 'http', capabilities: httpCapabilities };
  }
  return { kind: 'mock', capabilities: null };
}

export function transportCapabilities(): TransportCapabilities | null {
  return httpCapabilities;
}

function classifyError(message: string): DesktopBridgeError['kind'] {
  const value = message.toLowerCase();
  // "No project" must be tested before "parse": the not-a-task-file message
  // mentions frontmatter, and a fresh launch must never read as a broken file.
  if (value.includes('no project') || value.includes('not a todo.md task file') || value.includes('not task file') || value.includes('未打开')) return 'no_project';
  if (value.includes('rebase') || value.includes('conflict') || value.includes('外部更新')) return 'conflict';
  if (value.includes('lock') || value.includes('timeout') || value.includes('busy')) return 'locked';
  if (value.includes('parse') || value.includes('marker') || value.includes('frontmatter')) return 'parse';
  return 'unknown';
}

function asBridgeError(value: unknown): DesktopBridgeError {
  if (value instanceof DesktopBridgeError) return value;
  if (value instanceof HttpError && value.status === 409) return new DesktopBridgeError(value.message, 'conflict');
  if (value instanceof Error) return new DesktopBridgeError(value.message);
  if (typeof value === 'string') return new DesktopBridgeError(value);
  return new DesktopBridgeError('桌面后端返回了未知错误');
}

function normalizeTask(raw: unknown): DesktopTask {
  const task = (raw ?? {}) as Partial<DesktopTask>;
  const priority = task.priority === 1 || task.priority === 2 || task.priority === 3 || task.priority === 4 ? task.priority : null;
  return {
    id: typeof task.id === 'string' ? task.id : null,
    body: typeof task.body === 'string' ? task.body : '',
    completed: Boolean(task.completed),
    column: typeof task.column === 'string' ? task.column : null,
    indent_level: typeof task.indent_level === 'number' ? task.indent_level : 0,
    parent_id: typeof task.parent_id === 'string' ? task.parent_id : null,
    due: typeof task.due === 'string' ? task.due : null,
    priority,
    tags: Array.isArray(task.tags) ? task.tags.filter((tag): tag is string => typeof tag === 'string') : [],
    // Only 1-99 is a valid partial state; anything else is treated as absent so a
    // malformed value cannot render as a progress bar that claims 0% or 100%.
    progress:
      typeof task.progress === 'number' && task.progress >= 1 && task.progress <= 99
        ? task.progress
        : null,
    failed_on: typeof task.failed_on === 'string' ? task.failed_on : null,
    note: typeof task.note === 'string' && task.note.length > 0 ? task.note : null,
    evidence: typeof task.evidence === 'string' && task.evidence.length > 0 ? task.evidence : null,
    evidence_availability:
      task.evidence_availability?.status === 'available'
      && typeof task.evidence_availability.from === 'string'
      && typeof task.evidence_availability.to === 'string'
        ? {
            status: 'available',
            from: task.evidence_availability.from,
            to: task.evidence_availability.to,
          }
        : {
            status: 'unavailable',
            reason:
              task.evidence_availability?.status === 'unavailable'
              && typeof task.evidence_availability.reason === 'string'
                ? task.evidence_availability.reason
                : '当前任务没有可用的 Git 物证区间。',
          },
  };
}

function normalizeSnapshot(raw: unknown): DesktopSnapshot {
  const value = (raw ?? {}) as Partial<DesktopSnapshot>;
  if (typeof value.project_path !== 'string' || typeof value.revision !== 'string') {
    throw new DesktopBridgeError('桌面后端返回的 Snapshot 不完整');
  }
  const sources = Array.isArray(value.sources) ? value.sources : undefined;
  const projectTask = (rawTask: unknown): DesktopTask => {
    const raw = rawTask as Partial<DesktopTask>;
    const task = normalizeTask(raw);
    const source = typeof raw.source_path === 'string' ? raw.source_path : undefined;
    if (source === undefined) return { ...task, ...(sources ? { source_writable: false } : {}) };
    const entry = sources?.find((item) => item.path === source);
    const localId = raw.local_id ?? task.id;
    return {
      ...task, source_path: source, local_id: localId,
      id: localId === null ? null : sourceKey(source, localId),
      column: task.column === null ? null : sourceKey(source, task.column),
      parent_id: task.parent_id === null ? null : sourceKey(source, task.parent_id),
      source_writable: raw.source_writable !== false && entry?.status === 'managed' && entry.writable,
    };
  };
  const tasks = Array.isArray(value.tasks) ? value.tasks.map(projectTask) : [];
  const byId = new Map(tasks.filter((task) => task.id).map((task) => [task.id, task]));
  const normalizeGroup = (items: unknown): DesktopTask[] =>
    Array.isArray(items)
      ? items.map(projectTask).map((task) => (task.id ? byId.get(task.id) ?? task : task))
      : [];
  return {
    project_path: value.project_path,
    ...(sources ? { sources, selected_source: value.selected_source ?? null, warnings: value.warnings ?? [] } : {}),
    revision: value.revision,
    completion_policy: value.completion_policy === 'strict' ? 'strict' : 'normal',
    columns: Array.isArray(value.columns)
      ? value.columns.map((column) => {
          const item = column as Partial<DesktopColumn>;
          return {
            id: sourceKey(item.source_path, String(item.id ?? '')),
            ...(item.source_path === undefined ? {} : { source_path: item.source_path, local_id: String(item.id ?? '') }),
            name: String(item.name ?? item.id ?? ''),
            done: Boolean(item.done),
          };
        })
      : [],
    tasks,
    today: {
      overdue: normalizeGroup(value.today?.overdue),
      today: normalizeGroup(value.today?.today),
      upcoming: normalizeGroup(value.today?.upcoming),
      next: normalizeGroup(value.today?.next),
    },
  };
}

function normalizeCapture(raw: unknown): Capture {
  const capture = (raw ?? {}) as Partial<Capture>;
  return {
    id: String(capture.id ?? ''),
    created_at: String(capture.created_at ?? ''),
    text: String(capture.text ?? ''),
  };
}

const AI_STATES: ReadonlySet<string> = new Set(['running', 'waiting', 'unread', 'seen', 'failed', 'handled']);
const AI_OUTCOMES: ReadonlySet<string> = new Set(['completed', 'error', 'aborted', 'blocked', 'max-tokens', 'interrupted']);

function normalizeAiRecord(raw: unknown): AiRecord {
  const record = (raw ?? {}) as Partial<AiRecord>;
  if (typeof record.source !== 'string' || typeof record.session_id !== 'string' || typeof record.state !== 'string' || !AI_STATES.has(record.state)) {
    throw new DesktopBridgeError('AI 会话记录不完整');
  }
  const text = (value: unknown): string | null => (typeof value === 'string' && value.length > 0 ? value : null);
  const files = Array.isArray(record.changed_files) ? record.changed_files.filter((item): item is string => typeof item === 'string') : [];
  return {
    source: record.source,
    session_id: record.session_id,
    workspace: typeof record.workspace === 'string' ? record.workspace : '',
    state: record.state as AiState,
    outcome: typeof record.outcome === 'string' && AI_OUTCOMES.has(record.outcome) ? (record.outcome as AiOutcome) : null,
    title: text(record.title),
    question: text(record.question),
    reply: text(record.reply),
    reply_truncated: record.reply_truncated === true,
    changed_files: files,
    changed_file_count: typeof record.changed_file_count === 'number' ? Math.max(record.changed_file_count, files.length) : files.length,
    open_url: text(record.open_url),
    event_at: typeof record.event_at === 'number' ? record.event_at : 0,
    received_at: typeof record.received_at === 'number' ? record.received_at : 0,
    acked_at: typeof record.acked_at === 'number' ? record.acked_at : null,
    hidden: record.hidden === true,
    task_id: text(record.task_id),
  };
}

function normalizeAiList(raw: unknown): AiListResult {
  const value = (raw ?? {}) as { records?: unknown; connected?: unknown };
  if (!Array.isArray(value.records)) throw new DesktopBridgeError('ai_list 没有返回记录列表');
  return { records: value.records.map(normalizeAiRecord), connected: value.connected === true };
}

function cloneTasks(tasks: DesktopTask[]): DesktopTask[] {
  return tasks.map((task) => ({ ...task, tags: [...task.tags] }));
}

function deriveMockToday(tasks: DesktopTask[]): TodayView {
  const upcomingEnd = '2026-09-07';
  const incomplete = tasks.filter((task) => !task.completed);
  const active = incomplete.filter((task) => task.due);
  return {
    overdue: active.filter((task) => task.due! < MOCK_TODAY),
    today: active.filter((task) => task.due === MOCK_TODAY),
    upcoming: active.filter((task) => task.due! > MOCK_TODAY && task.due! <= upcomingEnd),
    next: incomplete.filter((task) => !task.due),
  };
}

function mockSnapshot(): DesktopSnapshot {
  const tasks = mockMode === 'empty' ? [] : cloneTasks(mockTasks);
  return {
    project_path: '/Users/demo/field-notes/TODO.md',
    revision: `mock-${String(mockRevision).padStart(4, '0')}`,
    completion_policy:
      typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('policy') === 'strict'
        ? 'strict'
        : 'normal',
    columns: mockColumns.map((column) => ({ ...column })),
    tasks,
    today: deriveMockToday(tasks),
  };
}

function mockRewrite(): ProposedRewrite {
  const hunks: DiffHunk[] = [
    { left_line: 1, right_line: 1, kind: 'context', text: '## Todo' },
    { left_line: 2, right_line: null, kind: 'removed', text: '- [ ] 旧的一行 <!--t:aaaaaa-->' },
    {
      left_line: null,
      right_line: 2,
      kind: 'added',
      text: '- [x] 旧的一行 <!--t:aaaaaa--> ✅2026-09-01',
    },
  ];
  return {
    diff: { hunks, added: 1, removed: 1 },
    proposed: '（mock 模式不写文件）',
    base_hash: 'mockbase00000000',
  };
}

function mockScanPreview(
  markdown: boolean,
  comments: boolean,
  importInto: string | null,
): ScanPreview {
  const report: ScanReport = {
    loose_checkboxes: markdown
      ? [{ path: 'docs/plan.md', line: 12, text: '写迁移脚本', completed: false }]
      : [],
    code_comments: comments
      ? [{ path: 'src/router.rs', line: 88, marker: 'TODO', text: '给登录接口加限流' }]
      : [],
    stats: { files_scanned: 114, files_skipped: 2, duplicates_filtered: 3 },
  };
  const empty = report.loose_checkboxes.length + report.code_comments.length === 0;
  return { report, rewrite: importInto && !empty ? mockRewrite() : null };
}

function mockHistory(): HistoryEntry[] {
  return [
    { version: 3, at: '2026-09-01T10:24:02+08:00', file: 'TODO.md', op: 'progress', bytes: 286, blob: 'v000003.snap' },
    { version: 2, at: '2026-09-01T10:23:41+08:00', file: 'TODO.md', op: 'board-move', bytes: 264, blob: 'v000002.snap' },
    { version: 1, at: '2026-09-01T10:22:19+08:00', file: 'TODO.md', op: 'add', bytes: 210, blob: 'v000001.snap' },
  ];
}

function emitMock(event: DesktopEvent): void {
  window.setTimeout(() => {
    mockListeners.forEach((listener) => listener(event));
  }, 24);
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asBridgeError(error);
  }
}

async function mockMutate(intent: DesktopMutation): Promise<DesktopSnapshot> {
  if (intent.type === 'add_task') {
    const id = `mck${String(mockTaskSequence++).padStart(3, '0')}`;
    mockTasks.push({
      id,
      body: intent.body.trim(),
      completed: false,
      column: intent.column ?? mockColumns[0]?.id ?? null,
      indent_level: 0,
      parent_id: null,
      due: null,
      priority: null,
      tags: [],
      progress: null,
      failed_on: null,
      note: null,
      evidence: null,
      evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
    });
  } else if (intent.type === 'set_completed') {
    mockTasks = mockTasks.map((task) =>
      task.id === intent.id
        ? { ...task, completed: intent.completed, note: intent.note?.trim() || task.note }
        : task,
    );
  } else if (intent.type === 'set_partial') {
    mockTasks = mockTasks.map((task) => task.id === intent.id
      ? { ...task, progress: intent.percent, note: intent.note.trim() }
      : task);
  } else if (intent.type === 'set_failed') {
    mockTasks = mockTasks.map((task) => task.id === intent.id
      ? { ...task, failed_on: MOCK_TODAY, note: intent.note.trim() }
      : task);
  } else if (intent.type === 'set_body') {
    const body = intent.body.trim();
    if (!body) throw new DesktopBridgeError('任务正文不能为空');
    mockTasks = mockTasks.map((task) => task.id === intent.id ? { ...task, body } : task);
  } else if (intent.type === 'set_priority') {
    const root = mockTasks.findIndex((task) => task.id === intent.id);
    const rootTask = mockTasks[root];
    const end = intent.scope === 'subtree' && rootTask
      ? mockTasks.findIndex((task, index) => index > root && (
          task.indent_level <= rootTask.indent_level || task.column !== rootTask.column
        ))
      : root + 1;
    const boundary = end === -1 ? mockTasks.length : end;
    mockTasks = mockTasks.map((task, index) =>
      index >= root && index < boundary ? { ...task, priority: intent.priority } : task);
  } else {
    const moving = mockTasks.find((task) => task.id === intent.id);
    if (moving) {
      const completed = Boolean(mockColumns.find((column) => column.id === intent.column)?.done);
      const enteringDone = completed && !moving.completed;
      const moved = {
        ...moving,
        column: intent.column,
        completed,
        progress: completed ? null : moving.progress,
        failed_on: completed ? null : moving.failed_on,
        note: completed
          ? intent.note === undefined
            ? enteringDone ? null : moving.note
            : intent.note.trim() || null
          : moving.note,
        evidence: intent.evidence ? `${intent.evidence.from}..${intent.evidence.to}` : moving.evidence,
      };
      const rest = mockTasks.filter((task) => task.id !== intent.id);
      // Mirror the file's ordering semantics: column_start lands ahead of the
      // column's first task, everything else at the end of the list.
      if (intent.target.type === 'column_start') {
        const firstInColumn = rest.findIndex((task) => task.column === intent.column);
        rest.splice(firstInColumn === -1 ? rest.length : firstInColumn, 0, moved);
      } else {
        rest.push(moved);
      }
      mockTasks = rest;
    }
  }
  mockRevision += 1;
  const snapshot = mockSnapshot();
  emitMock({ type: 'snapshot', snapshot });
  return snapshot;
}

function normalizeStatus(raw: unknown): DesktopStatus {
  const status = (raw ?? {}) as {
    startup_ms?: unknown;
    shortcut_status?: { status?: unknown; message?: unknown };
  };
  const startupMs = status.startup_ms;
  const shortcut = status.shortcut_status;
  if (startupMs !== null && typeof startupMs !== 'number') {
    throw new DesktopBridgeError('desktop_status.startup_ms 格式无效');
  }
  if (shortcut?.status === 'registered' || shortcut?.status === 'pending') {
    return { startup_ms: startupMs, shortcut_status: { status: shortcut.status } };
  }
  if (shortcut?.status === 'error' && typeof shortcut.message === 'string') {
    return {
      startup_ms: startupMs,
      shortcut_status: { status: 'error', message: shortcut.message },
    };
  }
  throw new DesktopBridgeError('desktop_status.shortcut_status 格式无效');
}

type CommandValue<T> =
  | { status: 'available'; value: T }
  | { status: 'unavailable'; message: string };

function readCommandValue<T>(raw: unknown, normalize: (value: unknown) => T): { value?: T; error?: string } {
  const commandValue = raw as Partial<CommandValue<unknown>> | null;
  if (commandValue?.status === 'available' && 'value' in commandValue) {
    return { value: normalize(commandValue.value) };
  }
  if (commandValue?.status === 'unavailable') {
    return { error: typeof commandValue.message === 'string' ? commandValue.message : '后端结果暂时不可用' };
  }
  return { error: '后端返回了未知的 CommandValue' };
}

export const desktop = {
  isTauri: isTauriRuntime(),

  currentWindowLabel(): string {
    if (isTauriRuntime()) return getCurrentWindow().label;
    return new URLSearchParams(window.location.search).get('window') === 'capture' ? 'capture' : 'main';
  },

  async hideCurrentWindow(): Promise<void> {
    if (isTauriRuntime()) await getCurrentWindow().hide();
  },

  async status(): Promise<DesktopStatus> {
    if (useHttp()) return { startup_ms: null, shortcut_status: { status: 'error', message: '浏览器不提供全局快捷键，请使用桌面版。' } };
    if (!isTauriRuntime()) {
      return {
        startup_ms: 86,
        shortcut_status: { status: 'registered' },
      };
    }
    return normalizeStatus(await call<unknown>('desktop_status'));
  },

  async snapshot(sourcePath: string | null = null, looseSources: string[] = []): Promise<DesktopSnapshot> {
    if (isTauriRuntime()) {
      const plain = sourcePath === null && looseSources.length === 0;
      return normalizeSnapshot(await call<unknown>(plain ? 'snapshot' : 'project_snapshot', plain ? undefined : { sourcePath, looseSources }));
    }
    if (useHttp()) {
      const base = sourceQuery('/snapshot', sourcePath);
      const query = looseSources.length ? `${base}${base.includes('?') ? '&' : '?'}loose=${encodeURIComponent(looseSources.join('\n'))}` : base;
      return normalizeSnapshot(await api<unknown>(query));
    }
    if (mockMode === 'no-project') throw new DesktopBridgeError('no project is open');
    if (mockMode === 'load-error') throw new DesktopBridgeError('frontmatter parse failed near todo-spec');
    return mockSnapshot();
  },

  async openProject(path: string): Promise<DesktopSnapshot> {
    if (isTauriRuntime()) return normalizeSnapshot(await call<unknown>('open_project', { path }));
    throw new DesktopBridgeError('浏览器不能打开本机目录，请使用桌面端或连接目标仓库的 todo serve 项目服务。');
  },

  /**
   * Scans the repository. Passing `importInto` also prepares the rewrite that
   * importing would produce; nothing is written either way.
   */
  async scanRepository(
    markdown: boolean,
    comments: boolean,
    importInto: string | null,
    sourcePath: string | null = null,
  ): Promise<ScanPreview> {
    if (useHttp()) {
      const report = await api<ScanReport>(
        sourceQuery(`/scan?markdown=${markdown}&comments=${comments}`, sourcePath),
      );
      const empty = report.loose_checkboxes.length + report.code_comments.length === 0;
      if (!importInto || empty) return { report, rewrite: null };
      const rewrite = await api<ProposedRewrite>(sourceQuery('/scan/import', sourcePath), {
        method: 'POST',
        body: { column: importInto, markdown, comments, source_path: sourcePath },
      });
      return { report, rewrite };
    }
    if (!isTauriRuntime()) return mockScanPreview(markdown, comments, importInto);
    return await call<ScanPreview>('scan_repository', {
      markdown,
      comments,
      importInto,
      sourcePath,
    });
  },

  /** Writes a rewrite the user has confirmed. Refused if the file moved on. */
  async applyRewrite(rewrite: ProposedRewrite, op: string, sourcePath: string | null = null, column = 'todo', version?: number, projectPath?: string): Promise<DesktopSnapshot> {
    if (useHttp()) {
      // The server recomputes the rewrite and checks the echoed hash, so a stale
      // confirmation is refused rather than silently applied.
      const path = op === 'restore' ? (version === undefined ? null : `/history/${version}/restore`) : '/scan/import';
      if (!path) throw new DesktopBridgeError('请从历史列表发起恢复');
      return normalizeSnapshot(await api<unknown>(sourceQuery(path, sourcePath), {
        method: 'POST', body: { column, source_path: sourcePath, confirm_base_hash: rewrite.base_hash },
      }));
    }
    if (!isTauriRuntime()) return mockSnapshot();
    return normalizeSnapshot(
      await call<unknown>('apply_rewrite', {
        proposed: rewrite.proposed,
        baseHash: rewrite.base_hash,
        op,
        sourcePath,
        projectPath,
      }),
    );
  },

  async historyList(sourcePath: string | null = null): Promise<HistoryEntry[]> {
    if (useHttp()) return await api<HistoryEntry[]>(sourceQuery('/history', sourcePath));
    if (!isTauriRuntime()) return mockHistory();
    return await call<HistoryEntry[]>('history_list', { sourcePath });
  },

  async historyPreview(version: number, sourcePath: string | null = null): Promise<ProposedRewrite> {
    if (useHttp()) return await api<ProposedRewrite>(sourceQuery(`/history/${version}/diff`, sourcePath));
    if (!isTauriRuntime()) return mockRewrite();
    return await call<ProposedRewrite>('history_preview', { version, sourcePath });
  },

  async alignmentPrompt(sourcePath: string | null = null): Promise<string> {
    if (useHttp()) {
      const response = await fetch(sourceQuery(`${apiBase()}/align/prompt`, sourcePath));
      if (!response.ok) throw new DesktopBridgeError('读取对齐提示词失败');
      return await response.text();
    }
    if (!isTauriRuntime()) return '# 任务：核对 TODO 与代码现实\n\n（mock 模式下的占位提示词）\n';
    return await call<string>('alignment_prompt', { sourcePath });
  },

  async chooseAndOpenProject(): Promise<DesktopSnapshot | null> {
    if (!isTauriRuntime()) {
      throw new DesktopBridgeError('浏览器不能打开本机目录，请使用桌面端或连接目标仓库的 todo serve 项目服务。');
    }
    const selected = await open({ directory: true, multiple: false, title: '选择项目根目录' });
    return typeof selected === 'string' ? this.openProject(selected) : null;
  },

  async mutate(intent: DesktopMutation, sourcePath: string | null = null, projectMode = false, projectPath?: string): Promise<DesktopSnapshot> {
    if (isTauriRuntime()) return normalizeSnapshot(await call<unknown>(projectMode || sourcePath !== null ? 'project_mutate' : 'mutate', projectMode || sourcePath !== null ? { sourcePath, intent, projectPath } : { intent }));
    if (useHttp() && projectMode) {
      if (!httpCapabilities?.writable) throw new DesktopBridgeError('服务端处于只读模式，无法修改任务');
      return normalizeSnapshot(await api<unknown>('/mutate', { method: 'POST', body: { source_path: sourcePath, intent } }));
    }
    if (useHttp()) {
      // Refuse locally rather than letting each write fail at the server: the UI
      // already knows the backend is read-only and can say so once.
      if (!httpCapabilities?.writable) {
        throw new DesktopBridgeError('服务端处于只读模式，无法修改任务');
      }
      const route =
        intent.type === 'add_task'
          ? { path: '/tasks', payload: { body: intent.body, column: intent.column } }
          : intent.type === 'set_completed'
            ? {
                path: `/tasks/${intent.id}/completed`,
                payload: {
                  completed: intent.completed,
                  ...(intent.note === undefined ? {} : { note: intent.note }),
                  ...(intent.evidence === undefined ? {} : { evidence: intent.evidence }),
                },
              }
            : intent.type === 'set_partial'
              ? { path: `/tasks/${intent.id}/progress`, payload: { percent: intent.percent, note: intent.note } }
              : intent.type === 'set_failed'
                ? { path: `/tasks/${intent.id}/fail`, payload: { note: intent.note } }
                : intent.type === 'set_body'
                  ? { path: `/tasks/${intent.id}/body`, payload: { body: intent.body } }
                : intent.type === 'set_priority'
                  ? {
                      path: `/tasks/${intent.id}/priority`,
                      payload: { priority: intent.priority, scope: intent.scope ?? 'task' },
                    }
                  : {
                    path: `/tasks/${intent.id}/move`,
                    payload: {
                      column: intent.column,
                      ...(intent.note === undefined ? {} : { note: intent.note }),
                      ...(intent.evidence === undefined ? {} : { evidence: intent.evidence }),
                    },
                  };
      return normalizeSnapshot(
        await api<unknown>(route.path, { method: 'POST', body: route.payload }),
      );
    }
    return mockMutate(intent);
  },

  async previewAdopt(sourcePath: string, loose = false): Promise<AdoptPreview> {
    if (httpCapabilities?.writable === false) throw new DesktopBridgeError('服务端只读，不能接管文件');
    if (isTauriRuntime()) return call<AdoptPreview>('preview_adopt', { sourcePath, loose });
    if (useHttp()) return api<AdoptPreview>('/sources/adopt/preview', { method: 'POST', body: { source_path: sourcePath, loose } });
    throw new DesktopBridgeError('演示模式不能接管真实文件。');
  },

  async applyAdopt(sourcePath: string, baseHash: string, projectPath?: string, loose = false): Promise<DesktopSnapshot> {
    if (httpCapabilities?.writable === false) throw new DesktopBridgeError('服务端只读，不能接管文件');
    if (isTauriRuntime()) return normalizeSnapshot(await call<unknown>('apply_adopt', { sourcePath, baseHash, projectPath, loose }));
    if (useHttp()) return normalizeSnapshot(await api<unknown>('/sources/adopt', { method: 'POST', body: { source_path: sourcePath, base_hash: baseHash, loose } }));
    throw new DesktopBridgeError('演示模式不能接管真实文件。');
  },

  async columnSetupOptions(sourcePath: string, expectedProjectPath: string): Promise<ColumnSetupOptions> {
    try {
      if (isTauriRuntime()) return readColumnSetupOptions(await call<unknown>('column_setup_options', { sourcePath, expectedProjectPath }), sourcePath);
      if (useHttp()) {
        const path = `${sourceQuery('/sources/columns', sourcePath)}&expected_project_path=${encodeURIComponent(expectedProjectPath)}`;
        return readColumnSetupOptions(await api<unknown>(path), sourcePath);
      }
      throw new DesktopBridgeError('演示模式不支持配置任务列，请连接真实项目服务或使用桌面版。');
    } catch (error) { throw asBridgeError(error); }
  },

  async previewColumns(sourcePath: string, selection: ColumnSetupSelection, expectedProjectPath: string): Promise<ColumnSetupPreview> {
    try {
      if (httpCapabilities?.writable === false) throw new DesktopBridgeError('服务端只读，不能配置任务列。');
      if (isTauriRuntime()) return readColumnSetupPreview(await call<unknown>('preview_columns', { sourcePath, selection, expectedProjectPath }), sourcePath, selection);
      if (useHttp()) return readColumnSetupPreview(await api<unknown>('/sources/columns/preview', {
        method: 'POST', body: { source_path: sourcePath, selection, expected_project_path: expectedProjectPath },
      }), sourcePath, selection);
      throw new DesktopBridgeError('演示模式不支持配置任务列，请连接真实项目服务或使用桌面版。');
    } catch (error) { throw asBridgeError(error); }
  },

  async applyColumns(preview: ColumnSetupPreview, expectedProjectPath: string): Promise<DesktopSnapshot> {
    try {
      if (httpCapabilities?.writable === false) throw new DesktopBridgeError('服务端只读，不能配置任务列。');
      const { source_path, base_hash, selection } = preview;
      // Deliberately omit proposed/diff/columns: Rust recomputes the controlled edit.
      if (isTauriRuntime()) return normalizeSnapshot(await call<unknown>('apply_columns', {
        sourcePath: source_path, baseHash: base_hash, selection, expectedProjectPath,
      }));
      if (useHttp()) return normalizeSnapshot(await api<unknown>('/sources/columns', {
        method: 'POST', body: { source_path, base_hash, selection, expected_project_path: expectedProjectPath },
      }));
      throw new DesktopBridgeError('演示模式不支持配置任务列，请连接真实项目服务或使用桌面版。');
    } catch (error) { throw asBridgeError(error); }
  },

  async captureList(): Promise<Capture[]> {
    if (useHttp()) return [];
    if (!isTauriRuntime()) return mockCaptures.map((capture) => ({ ...capture }));
    const result = await call<unknown>('capture_list');
    if (!Array.isArray(result)) throw new DesktopBridgeError('capture_list 没有返回 Capture 列表');
    return result.map(normalizeCapture);
  },

  async captureAppend(text: string): Promise<CaptureAppendResult> {
    if (useHttp()) throw new DesktopBridgeError('HTTP 项目服务不开放个人随记，请使用桌面版。');
    if (!isTauriRuntime()) {
      const capture: Capture = {
        id: `cap-mock-${String(mockCaptureSequence++).padStart(2, '0')}`,
        created_at: '2026-08-31T10:30:00Z',
        text: text.trim(),
      };
      mockCaptures = [...mockCaptures, capture];
      return {
        capture: { ...capture },
        captures: mockCaptures.map((item) => ({ ...item })),
      };
    }
    const raw = (await call<unknown>('capture_append', { text })) as Record<string, unknown>;
    if (!Array.isArray(raw.captures)) throw new DesktopBridgeError('capture_append 没有返回 authoritative Capture 列表');
    return {
      capture: normalizeCapture(raw.capture),
      captures: raw.captures.map(normalizeCapture),
    };
  },

  async captureDelete(id: string): Promise<Capture[]> {
    if (useHttp()) throw new DesktopBridgeError('HTTP 项目服务不开放个人随记，请使用桌面版。');
    if (!isTauriRuntime()) {
      mockCaptures = mockCaptures.filter((capture) => capture.id !== id);
      return mockCaptures.map((capture) => ({ ...capture }));
    }
    const captures = await call<unknown>('capture_delete', { id });
    if (!Array.isArray(captures)) throw new DesktopBridgeError('capture_delete 没有返回 authoritative Capture 列表');
    return captures.map(normalizeCapture);
  },

  async captureTriage(id: string, column: string, sourcePath: string | null = null, projectPath?: string): Promise<TriageResult> {
    if (useHttp()) throw new DesktopBridgeError('HTTP 项目服务不开放个人随记，请使用桌面版。');
    if (!isTauriRuntime()) {
      const capture = mockCaptures.find((item) => item.id === id);
      if (!capture) throw new DesktopBridgeError('Capture 已不存在');
      const taskId = `tri${String(mockTaskSequence++).padStart(3, '0')}`;
      mockTasks.push({
        id: taskId,
        body: capture.text,
        completed: false,
        column,
        indent_level: 0,
        parent_id: null,
        due: null,
        priority: null,
        tags: ['capture'],
        progress: null,
        failed_on: null,
        note: null,
        evidence: null,
        evidence_availability: { status: 'unavailable', reason: 'Mock 任务没有 Git 物证。' },
      });
      mockRevision += 1;
      const partial = id === 'cap-b7k2';
      if (!partial) mockCaptures = mockCaptures.filter((item) => item.id !== id);
      const commandValues = {
        snapshot: mockSnapshot(),
        captures: mockCaptures.map((item) => ({ ...item })),
      };
      const result: TriageResult = partial
        ? {
            status: 'task_created_capture_retained',
            task_id: taskId,
            message: '任务已创建，但 Capture 文件删除时遇到锁竞争。',
            ...commandValues,
          }
        : { status: 'completed', task_id: taskId, ...commandValues };
      emitMock({ type: 'snapshot', snapshot: result.snapshot! });
      return result;
    }
    const raw = await call<Record<string, unknown>>('capture_triage', { id, column, sourcePath, projectPath });
    const taggedResult = (raw.result ?? {}) as Record<string, unknown>;
    if (typeof taggedResult.task_id !== 'string' || !taggedResult.task_id) {
      throw new DesktopBridgeError('capture_triage.result 缺少 task_id');
    }
    if (taggedResult.status !== 'completed' && taggedResult.status !== 'task_created_capture_retained') {
      throw new DesktopBridgeError('capture_triage.result.status 格式无效');
    }
    const snapshotValue = readCommandValue(raw.snapshot, normalizeSnapshot);
    const capturesValue = readCommandValue(raw.captures, (value) => {
      if (!Array.isArray(value)) throw new DesktopBridgeError('capture_triage 返回的 Capture 列表无效');
      return value.map(normalizeCapture);
    });
    const commandValues = {
      snapshot: snapshotValue.value,
      captures: capturesValue.value,
      snapshot_error: snapshotValue.error,
      captures_error: capturesValue.error,
    };
    if (taggedResult.status === 'task_created_capture_retained') {
      if (typeof taggedResult.message !== 'string') {
        throw new DesktopBridgeError('capture_triage partial result 缺少 message');
      }
      return {
        status: taggedResult.status,
        task_id: taggedResult.task_id,
        message: taggedResult.message,
        ...commandValues,
      };
    }
    return {
      status: 'completed',
      task_id: taggedResult.task_id,
      ...commandValues,
    };
  },

  // ── AI feed ──────────────────────────────────────────────────────────
  // Every record, including handled and hidden ones; the view applies the
  // pills and drops hidden records so one read feeds the badge and the list.

  async aiList(workspace: string | null = null): Promise<AiListResult> {
    if (isTauriRuntime()) return normalizeAiList(await call<unknown>('ai_list', { workspace }));
    if (useHttp()) {
      const query = workspace ? `&workspace=${encodeURIComponent(workspace)}` : '';
      return normalizeAiList(await api<unknown>(`/ai/sessions?all=1${query}`));
    }
    return {
      records: mockAiRecords.map((record) => ({ ...record, changed_files: [...record.changed_files] })),
      connected: mockAiMode !== 'none',
    };
  },

  async aiAck(source: string, session: string): Promise<AiRecord> {
    if (isTauriRuntime()) return normalizeAiRecord(await call<unknown>('ai_ack', { source, session }));
    if (useHttp()) {
      if (!httpCapabilities?.writable) throw new DesktopBridgeError('服务端处于只读模式，无法标记会话');
      return normalizeAiRecord(await api<unknown>(`/ai/sessions/${encodeURIComponent(source)}/${encodeURIComponent(session)}/ack`, { method: 'POST', body: {} }));
    }
    return mockAiModify(source, session, (record) => ({ ...record, acked_at: Date.now() }));
  },

  async aiRemove(source: string, session: string): Promise<AiRecord> {
    if (isTauriRuntime()) return normalizeAiRecord(await call<unknown>('ai_remove', { source, session }));
    if (useHttp()) {
      if (!httpCapabilities?.writable) throw new DesktopBridgeError('服务端处于只读模式，无法移除会话');
      return normalizeAiRecord(await api<unknown>(`/ai/sessions/${encodeURIComponent(source)}/${encodeURIComponent(session)}/remove`, { method: 'POST', body: {} }));
    }
    return mockAiModify(source, session, (record) => ({ ...record, hidden: true }));
  },

  async aiLink(source: string, session: string, taskId: string): Promise<AiRecord> {
    if (isTauriRuntime()) return normalizeAiRecord(await call<unknown>('ai_link', { source, session, taskId }));
    if (useHttp()) {
      if (!httpCapabilities?.writable) throw new DesktopBridgeError('服务端处于只读模式，无法关联任务');
      return normalizeAiRecord(await api<unknown>(`/ai/sessions/${encodeURIComponent(source)}/${encodeURIComponent(session)}/link`, { method: 'POST', body: { task_id: taskId } }));
    }
    return mockAiModify(source, session, (record) => ({ ...record, task_id: taskId }));
  },

  /**
   * Creates a Project Task from a session (body = title, else question) with
   * a `↳` note pointing back at it, then links the record. The backend does
   * the three writes in one call so a half-done result cannot be retried
   * into a duplicate task.
   */
  async aiToTask(source: string, session: string, column: string | null, sourcePath: string | null = null, projectPath?: string): Promise<AiToTaskResult> {
    if (isTauriRuntime()) {
      const raw = (await call<Record<string, unknown>>('ai_to_task', { source, session, column, sourcePath, projectPath })) ?? {};
      if (typeof raw.task_id !== 'string' || !raw.task_id) throw new DesktopBridgeError('ai_to_task 缺少 task_id');
      const snapshot = readCommandValue(raw.snapshot, normalizeSnapshot);
      return { task_id: raw.task_id, record: normalizeAiRecord(raw.record), snapshot: snapshot.value, snapshot_error: snapshot.error };
    }
    if (useHttp()) {
      if (!httpCapabilities?.writable) throw new DesktopBridgeError('服务端处于只读模式，无法创建任务');
      const raw = await api<{ task_id: string; record: unknown; snapshot: unknown }>(`/ai/sessions/${encodeURIComponent(source)}/${encodeURIComponent(session)}/to-task`, {
        method: 'POST',
        body: { column, source_path: sourcePath },
      });
      return { task_id: raw.task_id, record: normalizeAiRecord(raw.record), snapshot: normalizeSnapshot(raw.snapshot) };
    }
    const existing = mockAiRecords.find((record) => record.source === source && record.session_id === session);
    if (!existing) throw new DesktopBridgeError(`会话 ${source}/${session} 已不存在`);
    const body = (existing.title ?? existing.question ?? `会话 ${session}`).split(/\s+/).join(' ');
    const note = existing.open_url ? `来自 ${source} 会话 ${session}：${existing.open_url}` : `来自 ${source} 会话 ${session}`;
    const snapshot = await mockMutate({ type: 'add_task', body, column });
    const created = mockTasks.at(-1);
    if (!created?.id) throw new DesktopBridgeError('mock 任务创建失败');
    created.note = note;
    const record = mockAiModify(source, session, (item) => ({ ...item, task_id: created.id }));
    return { task_id: created.id, record, snapshot: { ...snapshot, tasks: cloneTasks(mockTasks) } };
  },

  /** Opens a web URL in the system browser; the shell decides how. */
  async openExternal(url: string): Promise<void> {
    if (!/^https?:\/\//.test(url)) throw new DesktopBridgeError('只能打开 http/https 链接');
    if (isTauriRuntime()) {
      await call<void>('open_external', { url });
      return;
    }
    window.open(url, '_blank', 'noopener,noreferrer');
  },

  /**
   * Opens a session link the way the 「打开会话」 setting asks.
   *
   * `reuse`: one place for DSH — a `dsh-session` webview window in Tauri
   * (created once, navigated afterwards), a named browser tab on the web.
   * `new-tab`: the system browser, a fresh tab every time. The mock also
   * records each call so a test can check which path a setting took.
   */
  async openSession(url: string, mode: OpenSessionMode): Promise<void> {
    if (!/^https?:\/\//.test(url)) throw new DesktopBridgeError('只能打开 http/https 链接');
    if (isTauriRuntime()) {
      await call<void>(mode === 'reuse' ? 'open_session_window' : 'open_external', { url });
      return;
    }
    if (!useHttp()) (window.__todoMockOpenSessions ??= []).push({ url, mode });
    if (mode === 'reuse-existing') {
      const channel = 'todo-dsh-firefox-v1';
      const requestId = crypto.randomUUID();
      const result = await new Promise<{ ok: boolean; error?: string }>((resolve) => {
        let settled = false;
        const finish = (value: { ok: boolean; error?: string }) => { if (!settled) { settled = true; window.removeEventListener('message', onMessage); resolve(value); } };
        const onMessage = (event: MessageEvent) => {
          if (event.source === window && event.data?.channel === channel && event.data?.type === 'open-session-result' && event.data.requestId === requestId) finish(event.data);
        };
        window.addEventListener('message', onMessage);
        window.postMessage({ channel, type: 'open-session', requestId, url, expiresAt: Date.now() + 1500 }, window.location.origin);
        window.setTimeout(() => finish({ ok: false, error: 'Firefox 扩展未响应' }), 2000);
      });
      if (result.ok) return;
      // Extension unavailable: retain the existing safe web reuse behavior.
      window.open(url, SESSION_TAB_NAME);
      // Fallback is a successful user-visible navigation, not an operation error.
      return;
    }
    if (mode === 'reuse') window.open(url, SESSION_TAB_NAME);
    else window.open(url, '_blank', 'noopener,noreferrer');
  },

  /** Fires when the AI store changes underneath the app (CLI writes, or mock actions). */
  async subscribeAi(listener: () => void): Promise<UnlistenFn> {
    if (!isTauriRuntime()) {
      mockAiListeners.add(listener);
      return () => mockAiListeners.delete(listener);
    }
    return listen<unknown>('desktop://ai-changed', () => listener());
  },

  async subscribe(listener: (event: DesktopEvent) => void): Promise<UnlistenFn> {
    if (!isTauriRuntime()) {
      mockListeners.add(listener);
      return () => mockListeners.delete(listener);
    }
    const unlistenSnapshot = await listen<unknown>('desktop://snapshot-changed', (event) => {
      try {
        listener({ type: 'snapshot', snapshot: normalizeSnapshot(event.payload) });
      } catch (error) {
        listener({ type: 'error', error: asBridgeError(error) });
      }
    });
    const unlistenError = await listen<unknown>('desktop://snapshot-error', (event) => {
      listener({ type: 'error', error: asBridgeError(event.payload) });
    });
    return () => {
      unlistenSnapshot();
      unlistenError();
    };
  },
};

export function shortProjectName(path: string): string {
  const normalized = path.replace(/\\/g, '/').replace(/\/TODO\.md$/i, '');
  return normalized.split('/').filter(Boolean).at(-1) ?? '当前项目';
}
