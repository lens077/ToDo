import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import {
  api,
  apiBase,
  detectTransport,
  probeHttp,
  type TransportCapabilities,
  type TransportKind,
} from './transport';

export type TaskPriority = 1 | 2 | 3 | 4 | null;
export type PriorityScope = 'task' | 'subtree';

export interface DesktopColumn {
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

export interface DesktopSnapshot {
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

export class DesktopBridgeError extends Error {
  readonly kind: 'conflict' | 'locked' | 'parse' | 'no_project' | 'unknown';

  constructor(message: string, kind = classifyError(message)) {
    super(message);
    this.name = 'DesktopBridgeError';
    this.kind = kind;
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
  const tasks = Array.isArray(value.tasks) ? value.tasks.map(normalizeTask) : [];
  const byId = new Map(tasks.filter((task) => task.id).map((task) => [task.id, task]));
  const normalizeGroup = (items: unknown): DesktopTask[] =>
    Array.isArray(items)
      ? items.map(normalizeTask).map((task) => (task.id ? byId.get(task.id) ?? task : task))
      : [];
  return {
    project_path: value.project_path,
    revision: value.revision,
    completion_policy: value.completion_policy === 'strict' ? 'strict' : 'normal',
    columns: Array.isArray(value.columns)
      ? value.columns.map((column) => {
          const item = column as Partial<DesktopColumn>;
          return {
            id: String(item.id ?? ''),
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
    if (!isTauriRuntime()) {
      return {
        startup_ms: 86,
        shortcut_status: { status: 'registered' },
      };
    }
    return normalizeStatus(await call<unknown>('desktop_status'));
  },

  async snapshot(): Promise<DesktopSnapshot> {
    if (isTauriRuntime()) return normalizeSnapshot(await call<unknown>('snapshot'));
    if (useHttp()) return normalizeSnapshot(await api<unknown>('/snapshot'));
    if (mockMode === 'no-project') throw new DesktopBridgeError('no project is open');
    if (mockMode === 'load-error') throw new DesktopBridgeError('frontmatter parse failed near todo-spec');
    return mockSnapshot();
  },

  async openProject(path: string): Promise<DesktopSnapshot> {
    if (isTauriRuntime()) return normalizeSnapshot(await call<unknown>('open_project', { path }));
    mockMode = 'default';
    return mockSnapshot();
  },

  /**
   * Scans the repository. Passing `importInto` also prepares the rewrite that
   * importing would produce; nothing is written either way.
   */
  async scanRepository(
    markdown: boolean,
    comments: boolean,
    importInto: string | null,
  ): Promise<ScanPreview> {
    if (useHttp()) {
      const report = await api<ScanReport>(
        `/scan?markdown=${markdown}&comments=${comments}`,
      );
      const empty = report.loose_checkboxes.length + report.code_comments.length === 0;
      if (!importInto || empty) return { report, rewrite: null };
      const rewrite = await api<ProposedRewrite>('/scan/import', {
        method: 'POST',
        body: { column: importInto, markdown, comments },
      });
      return { report, rewrite };
    }
    if (!isTauriRuntime()) return mockScanPreview(markdown, comments, importInto);
    return await call<ScanPreview>('scan_repository', {
      markdown,
      comments,
      importInto,
    });
  },

  /** Writes a rewrite the user has confirmed. Refused if the file moved on. */
  async applyRewrite(rewrite: ProposedRewrite, op: string): Promise<DesktopSnapshot> {
    if (useHttp()) {
      // The server recomputes the rewrite and checks the echoed hash, so a stale
      // confirmation is refused rather than silently applied.
      const path = op === 'restore' ? null : '/scan/import';
      if (path) {
        return normalizeSnapshot(
          await api<unknown>(path, {
            method: 'POST',
            body: { column: 'todo', confirm_base_hash: rewrite.base_hash },
          }),
        );
      }
      throw new DesktopBridgeError('请从历史列表发起恢复');
    }
    if (!isTauriRuntime()) return mockSnapshot();
    return normalizeSnapshot(
      await call<unknown>('apply_rewrite', {
        proposed: rewrite.proposed,
        baseHash: rewrite.base_hash,
        op,
      }),
    );
  },

  async historyList(): Promise<HistoryEntry[]> {
    if (useHttp()) return await api<HistoryEntry[]>('/history');
    if (!isTauriRuntime()) return mockHistory();
    return await call<HistoryEntry[]>('history_list');
  },

  async historyPreview(version: number): Promise<ProposedRewrite> {
    if (useHttp()) return await api<ProposedRewrite>(`/history/${version}/diff`);
    if (!isTauriRuntime()) return mockRewrite();
    return await call<ProposedRewrite>('history_preview', { version });
  },

  async alignmentPrompt(): Promise<string> {
    if (useHttp()) {
      const response = await fetch(`${apiBase()}/align/prompt`);
      return await response.text();
    }
    if (!isTauriRuntime()) return '# 任务：核对 TODO 与代码现实\n\n（mock 模式下的占位提示词）\n';
    return await call<string>('alignment_prompt');
  },

  async chooseAndOpenProject(): Promise<DesktopSnapshot | null> {
    if (!isTauriRuntime()) {
      mockMode = 'default';
      return mockSnapshot();
    }
    const selected = await open({ directory: true, multiple: false, title: '选择包含 TODO.md 的项目' });
    return typeof selected === 'string' ? this.openProject(selected) : null;
  },

  async mutate(intent: DesktopMutation): Promise<DesktopSnapshot> {
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
    if (!isTauriRuntime()) return mockMutate(intent);
    return normalizeSnapshot(await call<unknown>('mutate', { intent }));
  },

  async captureList(): Promise<Capture[]> {
    if (!isTauriRuntime()) return mockCaptures.map((capture) => ({ ...capture }));
    const result = await call<unknown>('capture_list');
    if (!Array.isArray(result)) throw new DesktopBridgeError('capture_list 没有返回 Capture 列表');
    return result.map(normalizeCapture);
  },

  async captureAppend(text: string): Promise<CaptureAppendResult> {
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
    if (!isTauriRuntime()) {
      mockCaptures = mockCaptures.filter((capture) => capture.id !== id);
      return mockCaptures.map((capture) => ({ ...capture }));
    }
    const captures = await call<unknown>('capture_delete', { id });
    if (!Array.isArray(captures)) throw new DesktopBridgeError('capture_delete 没有返回 authoritative Capture 列表');
    return captures.map(normalizeCapture);
  },

  async captureTriage(id: string, column: string): Promise<TriageResult> {
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
    const raw = (await call<unknown>('capture_triage', { id, column })) as Record<string, unknown>;
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
