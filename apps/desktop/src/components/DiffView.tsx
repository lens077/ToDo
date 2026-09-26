import { useMemo } from 'react';
import type { DiffHunk, TextDiff } from '../lib/desktop';

/**
 * A row in the side-by-side view.
 *
 * A removal and the addition that follows it are paired onto one row so a
 * replaced line reads as a single change. Unpaired removals and additions keep
 * their own row with the opposite side blank.
 */
interface DiffRow {
  left: DiffHunk | null;
  right: DiffHunk | null;
}

function pairHunks(hunks: DiffHunk[]): DiffRow[] {
  const rows: DiffRow[] = [];
  let index = 0;
  while (index < hunks.length) {
    const hunk = hunks[index];
    if (hunk.kind === 'context') {
      rows.push({ left: hunk, right: hunk });
      index += 1;
      continue;
    }
    if (hunk.kind === 'removed') {
      // Gather the full run of removals, then the run of additions, and zip them.
      const removed: DiffHunk[] = [];
      while (index < hunks.length && hunks[index].kind === 'removed') {
        removed.push(hunks[index]);
        index += 1;
      }
      const added: DiffHunk[] = [];
      while (index < hunks.length && hunks[index].kind === 'added') {
        added.push(hunks[index]);
        index += 1;
      }
      const height = Math.max(removed.length, added.length);
      for (let offset = 0; offset < height; offset += 1) {
        rows.push({ left: removed[offset] ?? null, right: added[offset] ?? null });
      }
      continue;
    }
    rows.push({ left: null, right: hunk });
    index += 1;
  }
  return rows;
}

/**
 * Collapses long unchanged stretches, keeping a few lines of context either side.
 *
 * A whole-file rewrite can be hundreds of lines; without this the changed rows
 * are buried and the user scrolls past what they are meant to be judging.
 */
function collapse(rows: DiffRow[], context = 3): (DiffRow | { skipped: number })[] {
  const changed = rows.map(
    (row) => row.left?.kind !== 'context' || row.right?.kind !== 'context',
  );
  const keep = rows.map((_, index) =>
    changed.slice(Math.max(0, index - context), index + context + 1).some(Boolean),
  );

  const output: (DiffRow | { skipped: number })[] = [];
  let run = 0;
  rows.forEach((row, index) => {
    if (keep[index]) {
      if (run > 0) {
        output.push({ skipped: run });
        run = 0;
      }
      output.push(row);
    } else {
      run += 1;
    }
  });
  if (run > 0) output.push({ skipped: run });
  return output;
}

function cellClass(hunk: DiffHunk | null, side: 'left' | 'right'): string {
  if (!hunk) return 'diff-cell diff-cell--empty';
  if (hunk.kind === 'context') return 'diff-cell';
  if (side === 'left' && hunk.kind === 'removed') return 'diff-cell diff-cell--removed';
  if (side === 'right' && hunk.kind === 'added') return 'diff-cell diff-cell--added';
  return 'diff-cell';
}

export interface DiffViewProps {
  diff: TextDiff;
  leftLabel: string;
  rightLabel: string;
  /** Keep every backend-supplied context line visible for a full-file review. */
  fullContext?: boolean;
}

export function DiffView({ diff, leftLabel, rightLabel, fullContext = false }: DiffViewProps) {
  const rows = useMemo(() => fullContext ? pairHunks(diff.hunks) : collapse(pairHunks(diff.hunks)), [diff, fullContext]);

  if (diff.added === 0 && diff.removed === 0) {
    return <p className="diff-empty">两侧内容相同，没有需要确认的改动。</p>;
  }

  return (
    <div className="diff" role="table" aria-label="左右对比">
      <div className="diff-head" role="row">
        <span role="columnheader">{leftLabel}</span>
        <span role="columnheader">{rightLabel}</span>
      </div>
      <div className="diff-body" tabIndex={fullContext ? 0 : undefined} role={fullContext ? 'rowgroup' : undefined} aria-label={fullContext ? '完整差异，可滚动查看' : undefined}>
        {rows.map((row, index) => {
          if ('skipped' in row) {
            return (
              <div className="diff-skip" key={`skip-${index}`} role="row">
                <span role="cell">省略 {row.skipped} 行未改动</span>
              </div>
            );
          }
          return (
            <div className="diff-row" key={`row-${index}`} role="row">
              <span className="diff-gutter" role="cell">
                {row.left?.left_line ?? ''}
              </span>
              <code className={cellClass(row.left, 'left')} role="cell">
                {row.left?.text ?? ''}
              </code>
              <span className="diff-gutter" role="cell">
                {row.right?.right_line ?? ''}
              </span>
              <code className={cellClass(row.right, 'right')} role="cell">
                {row.right?.text ?? ''}
              </code>
            </div>
          );
        })}
      </div>
      <p className="diff-summary">
        右侧新增 {diff.added} 行，左侧移除 {diff.removed} 行
      </p>
    </div>
  );
}
