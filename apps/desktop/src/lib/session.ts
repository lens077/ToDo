//! Session client (Phase 4). Sessions are Markdown files inside the project
//! (`.virgo/sessions/`); the mock keeps the same lifecycle contract: one
//! active session, finish collects evidence, resume reports outstanding work.

import { invoke } from '@tauri-apps/api/core';
import { isTauriRuntime } from './workspace';

export interface SessionSummary {
  id: string;
  goal: string;
  status: 'active' | 'completed' | 'interrupted';
  started_at: number;
  finished_at: number | null;
  next_actions: string[];
  task_ids: string[];
  file_name: string;
}

export interface ValidationEntry {
  command: string;
  result: string;
  evidence: string;
}

export interface SessionFinishInput {
  completed: string[];
  decisions: string[];
  validation: ValidationEntry[];
  unresolved: string[];
  next_actions: string[];
  task_ids: string[];
  context_for_next: string;
}

export interface LinkedTask {
  id: string;
  found: boolean;
  completed: boolean;
  column: string | null;
  body: string;
}

export interface SessionResume {
  latest: SessionSummary | null;
  summary_markdown: string;
  changed_files: string[];
  outstanding: string[];
  linked_tasks: LinkedTask[];
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

// ── Mock state ──────────────────────────────────────────────────────────────

const mockSessions: Record<string, SessionSummary[]> = {};
const mockChangedFiles = ['src/lib/workspace.ts', 'src/components/WorkspaceView.tsx'];

// Demo task index so the mock can resolve linked task IDs the same way the
// Rust side resolves them against the project root's TODO.md.
const mockTaskIndex: Record<string, { body: string; completed: boolean; column: string }> = {
  m0ckwatch: { body: '接入 watcher 增量扫描', completed: false, column: 'doing' },
  m0ckdone: { body: '已完成的示例任务', completed: true, column: 'done' },
};

export function resetSessionMock(): void {
  for (const key of Object.keys(mockSessions)) delete mockSessions[key];
}

function errorOf(message: string): Error {
  return { name: 'SessionError', message } as Error;
}

function mockActive(projectId: string): SessionSummary | undefined {
  return (mockSessions[projectId] ?? []).find((session) => session.status === 'active');
}

export const session = {
  async begin(projectId: string, goal: string): Promise<SessionSummary> {
    if (isTauriRuntime()) return call<SessionSummary>('session_begin', { projectId, goal });
    if (!goal.trim()) throw errorOf('会话目标不能为空');
    if (mockActive(projectId)) throw errorOf('已有进行中的会话；先完成或中断它再开始新会话');
    const now = Date.now();
    const created: SessionSummary = {
      id: `s-${now}`,
      goal: goal.trim(),
      status: 'active',
      started_at: now,
      finished_at: null,
      next_actions: [],
      task_ids: [],
      file_name: `s-${now}.md`,
    };
    (mockSessions[projectId] ??= []).unshift(created);
    return created;
  },

  async finish(projectId: string, input: SessionFinishInput): Promise<SessionSummary> {
    if (isTauriRuntime()) return call<SessionSummary>('session_finish', { projectId, input });
    const active = mockActive(projectId);
    if (!active) throw errorOf('没有进行中的会话');
    const now = Date.now();
    const finished: SessionSummary = {
      ...active,
      status: 'completed',
      finished_at: now,
      next_actions: input.next_actions,
      task_ids: [...new Set(input.task_ids.map((id) => id.trim()).filter(Boolean))],
    };
    mockSessions[projectId] = mockSessions[projectId].map((entry) =>
      entry.id === active.id ? finished : entry,
    );
    return finished;
  },

  async interrupt(projectId: string): Promise<SessionSummary> {
    if (isTauriRuntime()) return call<SessionSummary>('session_interrupt', { projectId });
    const active = mockActive(projectId);
    if (!active) throw errorOf('没有进行中的会话');
    const interrupted: SessionSummary = { ...active, status: 'interrupted', finished_at: Date.now() };
    mockSessions[projectId] = mockSessions[projectId].map((entry) =>
      entry.id === active.id ? interrupted : entry,
    );
    return interrupted;
  },

  async list(projectId: string): Promise<SessionSummary[]> {
    if (isTauriRuntime()) return call<SessionSummary[]>('session_list', { projectId });
    return [...(mockSessions[projectId] ?? [])];
  },

  async resume(projectId: string): Promise<SessionResume> {
    if (isTauriRuntime()) return call<SessionResume>('session_resume', { projectId });
    const sessions = mockSessions[projectId] ?? [];
    const latest = sessions[0] ?? null;
    if (!latest) {
      return { latest: null, summary_markdown: '（项目还没有会话记录）', changed_files: [], outstanding: [], linked_tasks: [] };
    }
    const linkedTasks: LinkedTask[] = latest.task_ids.map((id) => {
      const task = mockTaskIndex[id];
      return task
        ? { id, found: true, completed: task.completed, column: task.column, body: task.body }
        : { id, found: false, completed: false, column: null, body: '' };
    });
    const summaryMarkdown = [
      `# Session Summary`,
      ``,
      `## Goal`,
      ``,
      latest.goal,
      ``,
      `## Files Changed`,
      ``,
      ...mockChangedFiles.map((file) => `- ${file}`),
      ``,
      `## Next Actions`,
      ``,
      ...(latest.next_actions.length ? latest.next_actions.map((item) => `- ${item}`) : ['- （未记录）']),
      ``,
      `## Task Links`,
      ``,
      ...(latest.task_ids.length ? latest.task_ids.map((id) => `- ${id}`) : ['- （未记录）']),
    ].join('\n');
    return {
      latest,
      summary_markdown: summaryMarkdown,
      changed_files: [...mockChangedFiles],
      outstanding: latest.status === 'active' ? [latest.goal] : latest.next_actions,
      linked_tasks: linkedTasks,
    };
  },
};
