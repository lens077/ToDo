import type { TextDiff } from './desktop';

/** Controlled intent only; the proposed Markdown is never an apply payload. */
export type ColumnSetupSelection =
  | { mode: 'defaults' }
  | { mode: 'defaults_and_start'; task_id: string }
  | { mode: 'existing'; headings: Array<{ name: string; done: boolean }> };

export interface ColumnSetupOptions {
  source_path: string;
  base_hash: string;
  headings: Array<{ name: string; line: number; selectable: boolean; reason: string | null }>;
  configured: boolean;
}

export interface ColumnSetupPreview {
  source_path: string;
  base_hash: string;
  selection: ColumnSetupSelection;
  columns: Array<{ id: string; name: string; done: boolean; created: boolean }>;
  proposed: string;
  diff: TextDiff;
}

export function sameColumnSelection(left: ColumnSetupSelection, right: ColumnSetupSelection): boolean {
  if (left.mode !== right.mode) return false;
  if (left.mode === 'defaults') return true;
  if (left.mode === 'defaults_and_start') return right.mode === 'defaults_and_start' && left.task_id === right.task_id;
  return right.mode === 'existing' && left.headings.length === right.headings.length
    && left.headings.every((heading, index) => heading.name === right.headings[index].name && heading.done === right.headings[index].done);
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object';
}

function selection(value: unknown): value is ColumnSetupSelection {
  return record(value) && (value.mode === 'defaults' || (value.mode === 'defaults_and_start' && typeof value.task_id === 'string' && value.task_id.length > 0) || (value.mode === 'existing'
    && Array.isArray(value.headings) && value.headings.length > 0
    && value.headings.every((heading) => record(heading) && typeof heading.name === 'string' && typeof heading.done === 'boolean')));
}

function bound(value: unknown, sourcePath: string): value is Record<string, unknown> {
  return record(value) && value.source_path === sourcePath && typeof value.base_hash === 'string' && value.base_hash.length > 0;
}

export function readColumnSetupOptions(value: unknown, sourcePath: string): ColumnSetupOptions {
  if (!bound(value, sourcePath) || typeof value.configured !== 'boolean' || !Array.isArray(value.headings)
    || !value.headings.every((heading) => record(heading) && typeof heading.name === 'string'
      && Number.isInteger(heading.line) && Number(heading.line) > 0 && typeof heading.selectable === 'boolean'
      && (heading.reason === null || typeof heading.reason === 'string'))) {
    throw new Error('任务列选项不完整或来源不一致，请重新读取。');
  }
  return value as unknown as ColumnSetupOptions;
}

export function readColumnSetupPreview(value: unknown, sourcePath: string, expectedSelection: ColumnSetupSelection): ColumnSetupPreview {
  if (!bound(value, sourcePath) || !selection(value.selection) || !sameColumnSelection(value.selection, expectedSelection)
    || !Array.isArray(value.columns) || value.columns.length === 0
    || !value.columns.every((column) => record(column) && typeof column.id === 'string' && typeof column.name === 'string'
      && typeof column.done === 'boolean' && typeof column.created === 'boolean')
    || typeof value.proposed !== 'string' || !record(value.diff) || !Array.isArray(value.diff.hunks)
    || typeof value.diff.added !== 'number' || typeof value.diff.removed !== 'number'
    || !value.diff.hunks.every((hunk) => record(hunk) && ['context', 'added', 'removed'].includes(String(hunk.kind))
      && typeof hunk.text === 'string' && (hunk.left_line === null || Number.isInteger(hunk.left_line))
      && (hunk.right_line === null || Number.isInteger(hunk.right_line)))) {
    throw new Error('任务列预览不完整，或来源、选择不一致；请重新预览。');
  }
  return value as unknown as ColumnSetupPreview;
}
