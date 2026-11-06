/**
 * 阅读上下文 (Phase 6): why the article was saved, what to focus on later,
 * and its project/task links. The panel owns its edits and persists through
 * `reading.updateContext` — the only write path for these fields, so a
 * re-fetch can never clobber them (the Rust UPDATE doesn't mention them).
 */
import * as React from 'react';
import { reading, type ReadingArticleDetail, type ReadingContextInput } from '../lib/reading';

interface ReadingContextPanelProps {
  detail: ReadingArticleDetail;
  /** Swaps the open detail after a successful save. */
  onDetailChanged: (detail: ReadingArticleDetail) => void;
}

const FIELDS: Array<{ key: keyof ReadingContextInput; label: string; placeholder: string; rows: number }> = [
  {
    key: 'saved_reason',
    label: '收藏原因',
    placeholder: '为什么保存这篇文章？例如：研究 GitOps 部署流程时发现。',
    rows: 2,
  },
  {
    key: 'focus_questions',
    label: '未来关注点',
    placeholder: '以后阅读时重点回答什么问题？',
    rows: 2,
  },
  {
    key: 'revisit_condition',
    label: '复访条件',
    placeholder: '满足什么条件时重新阅读？',
    rows: 1,
  },
  {
    key: 'personal_notes',
    label: '个人笔记',
    placeholder: '阅读后的理解与结论。',
    rows: 3,
  },
];

export function ReadingContextPanel({ detail, onDetailChanged }: ReadingContextPanelProps) {
  const [context, setContext] = React.useState(detail.context);
  const [status, setStatus] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [expanded, setExpanded] = React.useState(
    Boolean(detail.context.saved_reason || detail.context.focus_questions || detail.context.personal_notes) ||
      detail.context.parse_status === 'failed',
  );
  const [linkDraft, setLinkDraft] = React.useState('');

  React.useEffect(() => {
    setContext(detail.context);
  }, [detail.context]);

  const save = React.useCallback(
    async (next: typeof context) => {
      setBusy(true);
      try {
        const saved = await reading.updateContext(detail.id, {
          saved_reason: next.saved_reason,
          focus_questions: next.focus_questions,
          revisit_condition: next.revisit_condition,
          personal_notes: next.personal_notes,
          revisit_type: next.revisit_type || 'NONE',
        });
        onDetailChanged(saved);
        setContext(saved.context);
        setStatus('上下文已保存');
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setBusy(false);
      }
    },
    [detail.id, onDetailChanged],
  );

  const refetch = React.useCallback(async () => {
    setBusy(true);
    try {
      const refetched = await reading.refetch(detail.id);
      onDetailChanged(refetched);
      setContext(refetched.context);
      setStatus('已重新解析（上下文与笔记保留）');
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, [detail.id, onDetailChanged]);

  const addLink = React.useCallback(
    async (kind: 'project' | 'task') => {
      const reference = linkDraft.trim();
      if (!reference) return;
      setBusy(true);
      try {
        const next =
          kind === 'project'
            ? await reading.linkProject(detail.id, reference, '')
            : await reading.linkTask(detail.id, reference, '');
        onDetailChanged(next);
        setContext(next.context);
        setLinkDraft('');
        setStatus('关联已保存');
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setBusy(false);
      }
    },
    [detail.id, linkDraft, onDetailChanged],
  );

  const removeLink = React.useCallback(
    async (kind: 'project' | 'task', reference: string) => {
      setBusy(true);
      try {
        const next =
          kind === 'project'
            ? await reading.unlinkProject(detail.id, reference)
            : await reading.unlinkTask(detail.id, reference);
        onDetailChanged(next);
        setContext(next.context);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setBusy(false);
      }
    },
    [detail.id, onDetailChanged],
  );

  return (
    <section className="rctx" data-testid="reading-context">
      <div className="rctx__head">
        <button type="button" className="rctx__toggle" aria-expanded={expanded} onClick={() => setExpanded((open) => !open)}>
          阅读上下文
          {detail.context.saved_reason && !expanded && (
            <span className="rctx__preview">{detail.context.saved_reason}</span>
          )}
        </button>
        <div className="rctx__tools">
          {detail.context.parse_status === 'failed' && (
            <span className="rctx__failed">解析失败 · URL 已保留</span>
          )}
          <button type="button" className="rctx__tool" disabled={busy} onClick={() => void refetch()}>
            重新解析
          </button>
        </div>
      </div>
      {expanded && (
        <div className="rctx__body">
          {FIELDS.map((field) => (
            <label className="rctx__field" key={field.key}>
              <span>{field.label}</span>
              <textarea
                rows={field.rows}
                placeholder={field.placeholder}
                value={context[field.key]}
                onChange={(event) => setContext({ ...context, [field.key]: event.target.value })}
                onBlur={() => {
                  if (context[field.key] !== detail.context[field.key]) void save(context);
                }}
              />
            </label>
          ))}
          <div className="rctx__links">
            <div className="rctx__link-group">
              <h5>关联项目</h5>
              <div className="rctx__chips">
                {context.project_links.map((link) => (
                  <span className="rctx__chip" key={link.reference}>
                    {link.label || link.reference}
                    <button type="button" aria-label={`移除关联 ${link.label || link.reference}`} onClick={() => void removeLink('project', link.reference)}>×</button>
                  </span>
                ))}
              </div>
            </div>
            <div className="rctx__link-group">
              <h5>关联任务</h5>
              <div className="rctx__chips">
                {context.task_links.map((link) => (
                  <span className="rctx__chip" key={link.reference}>
                    {link.label || link.reference}
                    <button type="button" aria-label={`移除关联 ${link.label || link.reference}`} onClick={() => void removeLink('task', link.reference)}>×</button>
                  </span>
                ))}
              </div>
            </div>
            <div className="rctx__add">
              <input
                value={linkDraft}
                placeholder="输入项目或任务 ID 关联（例如注册项目 ID / 任务 ID）"
                onChange={(event) => setLinkDraft(event.target.value)}
              />
              <button type="button" className="rctx__tool" disabled={busy || !linkDraft.trim()} onClick={() => void addLink('project')}>
                关联项目
              </button>
              <button type="button" className="rctx__tool" disabled={busy || !linkDraft.trim()} onClick={() => void addLink('task')}>
                关联任务
              </button>
              <button
                type="button"
                className="rctx__tool"
                disabled={busy}
                onClick={() => {
                  void (async () => {
                    setBusy(true);
                    try {
                      const markdown = await reading.exportReference(detail.id);
                      await navigator.clipboard.writeText(markdown);
                      setStatus('引用卡片已复制到剪贴板');
                      setError(null);
                    } catch (cause) {
                      setError(cause instanceof Error ? cause.message : String(cause));
                    } finally {
                      setBusy(false);
                    }
                  })();
                }}
              >
                导出引用
              </button>
            </div>
          </div>
          {status && <p className="rctx__status" role="status">{status}</p>}
          {error && <p className="rctx__status rctx__status--error" role="alert">{error}</p>}
        </div>
      )}
    </section>
  );
}
