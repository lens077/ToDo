//! Knowledge documents client (Phase 3). Mirrors `lib/workspace.ts`.
//!
//! The Rust index never moves or rewrites files; the reading view is rendered
//! fresh from disk on every open. The mock keeps the same contract so E2E can
//! exercise suggestion → confirm → read without a filesystem.

import { invoke } from '@tauri-apps/api/core';
import { isTauriRuntime } from './workspace';

export const LOGICAL_TYPES = [
  'general',
  'brief',
  'tasks',
  'progress',
  'architecture',
  'environment',
  'runbook',
  'agent_instructions',
  'decision',
  'implementation',
  'reference',
] as const;

export type LogicalType = (typeof LOGICAL_TYPES)[number];

export const LOGICAL_TYPE_LABELS: Record<LogicalType, string> = {
  general: '未分类',
  brief: '项目简介',
  tasks: '任务',
  progress: '进度',
  architecture: '架构',
  environment: '环境',
  runbook: '运行手册',
  agent_instructions: 'Agent 规则',
  decision: '决策',
  implementation: '实施方案',
  reference: '参考资料',
};

export interface KnowledgeDocument {
  id: string;
  project_id: string;
  relative_path: string;
  logical_type: LogicalType;
  title: string;
  classification_source: 'suggested' | 'confirmed';
  modified_at: number;
  indexed_at: number;
  tombstone: boolean;
}

export interface KnowledgeScanReport {
  scanned: number;
  indexed: number;
  unchanged: number;
  tombstoned: number;
  documents: KnowledgeDocument[];
}

export interface KnowledgeDocumentContent extends KnowledgeDocument {
  content_html: string;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

// ── Mock state ──────────────────────────────────────────────────────────────

interface MockFile {
  relative_path: string;
  markdown: string;
  confirmed_type: LogicalType | null;
}

const mockFilesByProject: Record<string, MockFile[]> = {};
let mockSequence = 1;

function mockSuggest(relativePath: string): LogicalType {
  const lower = relativePath.toLowerCase();
  const stem = lower.split('/').pop()!.replace(/\.md$/, '');
  if (stem === 'todo' || stem === 'tasks') return 'tasks';
  if (stem === 'readme' || stem === 'project') return 'brief';
  if (stem === 'changelog' || stem === 'progress') return 'progress';
  if (lower.includes('adr') || lower.includes('decision')) return 'decision';
  if (lower.includes('architecture')) return 'architecture';
  return 'general';
}

function mockTitle(markdown: string, relativePath: string): string {
  const heading = markdown.split('\n').map((line) => line.trim()).find((line) => line.startsWith('# '));
  return heading ? heading.slice(2).trim() : relativePath;
}

function mockDocument(projectId: string, file: MockFile): KnowledgeDocument {
  const suggested = mockSuggest(file.relative_path);
  return {
    id: `kdoc-${file.relative_path}`,
    project_id: projectId,
    relative_path: file.relative_path,
    logical_type: file.confirmed_type ?? suggested,
    title: mockTitle(file.markdown, file.relative_path),
    classification_source: file.confirmed_type ? 'confirmed' : 'suggested',
    modified_at: Date.now(),
    indexed_at: Date.now(),
    tombstone: false,
  };
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** Minimal Markdown → HTML for the mock: headings, bold, lists, code fences. */
function mockRender(markdown: string): string {
  const lines = markdown.split('\n');
  const out: string[] = [];
  let inCode = false;
  for (const line of lines) {
    if (line.startsWith('```')) {
      out.push(inCode ? '</code></pre>' : '<pre><code>');
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      out.push(escapeHtml(line));
      continue;
    }
    const heading = /^(#{1,6}) (.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      out.push(`<h${level}>${escapeHtml(heading[2])}</h${level}>`);
    } else if (/^\s*[-*] /.test(line)) {
      out.push(`<ul><li>${escapeHtml(line.replace(/^\s*[-*] /, ''))}</li></ul>`);
    } else if (line.trim() === '') {
      out.push('');
    } else {
      out.push(`<p>${escapeHtml(line).replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')}</p>`);
    }
  }
  return out.join('\n');
}

export function seedKnowledgeMock(projectId: string, files: Array<{ path: string; markdown: string }>): void {
  mockFilesByProject[projectId] = files.map((file) => ({
    relative_path: file.path,
    markdown: file.markdown,
    confirmed_type: null,
  }));
}

export function resetKnowledgeMock(): void {
  for (const key of Object.keys(mockFilesByProject)) delete mockFilesByProject[key];
  mockSequence = 1;
}

export const knowledge = {
  async scan(projectId: string): Promise<KnowledgeScanReport> {
    if (isTauriRuntime()) return call<KnowledgeScanReport>('knowledge_scan', { projectId });
    // Browser mock: seed a small demo corpus so the E2E can drive the flow
    // without a filesystem. Real projects always come from a real scan.
    const files = (mockFilesByProject[projectId] ??= [
      { relative_path: 'README.md', markdown: '# 演示项目\n\n目标与范围。\n', confirmed_type: null },
      { relative_path: 'docs/adr/0003-storage.md', markdown: '# ADR 3 选择 SQLite\n\n背景与后果。\n', confirmed_type: null },
      { relative_path: 'docs/notes.md', markdown: '# 随手记\n\n待整理。\n', confirmed_type: null },
    ]);
    return {
      scanned: files.length,
      indexed: files.length,
      unchanged: 0,
      tombstoned: 0,
      documents: files.map((file) => mockDocument(projectId, file)),
    };
  },

  async list(projectId: string): Promise<KnowledgeDocument[]> {
    if (isTauriRuntime()) return call<KnowledgeDocument[]>('knowledge_list', { projectId });
    const files = mockFilesByProject[projectId];
    return files ? files.map((file) => mockDocument(projectId, file)) : [];
  },

  async confirm(documentId: string, logicalType: LogicalType): Promise<KnowledgeDocument> {
    if (isTauriRuntime()) return call<KnowledgeDocument>('knowledge_confirm', { documentId, logicalType });
    if (!LOGICAL_TYPES.includes(logicalType)) throw { name: 'KnowledgeError', message: `未知文档类型: ${logicalType}` };
    for (const files of Object.values(mockFilesByProject)) {
      const file = files.find((candidate) => `kdoc-${candidate.relative_path}` === documentId);
      if (file) {
        file.confirmed_type = logicalType;
        return mockDocument('', file);
      }
    }
    throw { name: 'KnowledgeError', message: `文档不存在: ${documentId}` };
  },

  async read(documentId: string): Promise<KnowledgeDocumentContent> {
    if (isTauriRuntime()) return call<KnowledgeDocumentContent>('knowledge_read', { documentId });
    for (const files of Object.values(mockFilesByProject)) {
      const file = files.find((candidate) => `kdoc-${candidate.relative_path}` === documentId);
      if (file) {
        return { ...mockDocument('', file), content_html: mockRender(file.markdown) };
      }
    }
    throw { name: 'KnowledgeError', message: `文档不存在: ${documentId}` };
  },
};
