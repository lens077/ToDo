import * as React from 'react';
import type { AiFeedModel, AiTaskTarget } from '../hooks/useAiFeed';
import {
  AI_OUTCOME_LABEL,
  AI_PILL_ORDER,
  AI_STATE_LABEL,
  absoluteTime,
  canAck,
  countByState,
  displayPath,
  displayState,
  groupSections,
  recordKey,
  relativeTime,
  workspaceName,
  type AiRecord,
  type AiState,
} from '../lib/ai';
import type { DesktopSnapshot } from '../lib/desktop';
import { shortId } from '../lib/format';
import { Icon } from './Icon';

interface AiViewProps {
  feed: AiFeedModel;
  snapshot: DesktopSnapshot | null;
  /** The source new tasks go to when the project has several files. */
  newTaskSource: string | null;
  readOnly: boolean;
}

const VISIBLE_FILES = 3;

/**
 * Where 转为任务 writes: the first active column of the source new tasks go
 * to. Mirrors the 随记 triage default so the two paths land in the same list.
 */
function taskTarget(snapshot: DesktopSnapshot | null, newTaskSource: string | null): AiTaskTarget | null {
  if (!snapshot) return null;
  if (!snapshot.sources) {
    const column = snapshot.columns.find((item) => !item.done) ?? snapshot.columns[0];
    return { column: column?.id ?? null, sourcePath: null };
  }
  const writable = (path: string | null | undefined): boolean =>
    Boolean(snapshot.sources?.some((source) => source.path === path && source.status === 'managed' && source.writable));
  const source = [newTaskSource, snapshot.selected_source]
    .find((candidate): candidate is string => typeof candidate === 'string' && writable(candidate))
    ?? snapshot.sources.find((item) => item.status === 'managed' && item.writable)?.path;
  if (!source) return null;
  const columns = snapshot.columns.filter((item) => item.source_path === source);
  const column = columns.find((item) => !item.done) ?? columns[0];
  if (!column) return null;
  return { column: column.local_id ?? column.id, sourcePath: source };
}

/**
 * Whether a CSS line clamp is actually hiding something. Measured rather
 * than guessed from the character count, so the toggle appears exactly when
 * there is more to see: a card that is 270px wide and one that is 400px wide
 * clamp at different lengths. While expanded the last measurement stands, so
 * the 收起 button stays until the text is folded again.
 */
function useClampOverflow(ref: React.RefObject<HTMLElement | null>, expanded: boolean, text: string): boolean {
  const [overflowing, setOverflowing] = React.useState(false);
  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!element || expanded) return undefined;
    const measure = () => setOverflowing(element.scrollHeight > element.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref, expanded, text]);
  return overflowing;
}

function ClampToggle({ expanded, onToggle }: { expanded: boolean; onToggle: () => void }) {
  return (
    <button type="button" className="ai-card__toggle" aria-expanded={expanded} onClick={onToggle}>
      {expanded ? '收起' : '展开'}
    </button>
  );
}

/** The latest human question, clamped to four lines with the same 展开 as the reply. */
function QuestionPreview({ question }: { question: string }) {
  const [expanded, setExpanded] = React.useState(false);
  const ref = React.useRef<HTMLParagraphElement>(null);
  const long = useClampOverflow(ref, expanded, question);
  return (
    <div className="ai-card__question">
      <p ref={ref} className={expanded ? '' : 'is-clamped'}>
        <span className="ai-card__question-tag">提问</span>「{question}」
      </p>
      {long && <ClampToggle expanded={expanded} onToggle={() => setExpanded((value) => !value)} />}
    </div>
  );
}

function ReplyPreview({ record }: { record: AiRecord }) {
  const [expanded, setExpanded] = React.useState(false);
  const ref = React.useRef<HTMLParagraphElement>(null);
  const reply = record.reply ?? '';
  // Three clamped lines; the clamp itself is CSS and the toggle follows what
  // it hides.
  const long = useClampOverflow(ref, expanded, reply);
  if (!record.reply) return null;
  return (
    <div className="ai-card__reply">
      <p ref={ref} className={expanded ? '' : 'is-clamped'}>{reply}{record.reply_truncated && expanded ? <span className="ai-card__truncated">（来源已截断）</span> : null}</p>
      {long && <ClampToggle expanded={expanded} onToggle={() => setExpanded((value) => !value)} />}
    </div>
  );
}

function ChangedFiles({ record }: { record: AiRecord }) {
  if (record.changed_file_count === 0 && record.changed_files.length === 0) return null;
  const shown = record.changed_files.slice(0, VISIBLE_FILES);
  const rest = Math.max(record.changed_file_count, record.changed_files.length) - shown.length;
  return (
    <ul className="ai-card__files" aria-label={`改动 ${Math.max(record.changed_file_count, record.changed_files.length)} 个文件`}>
      {shown.map((file) => {
        const path = displayPath(file, record.workspace);
        return (
          <li key={file} title={path.title}><code>{path.label}</code></li>
        );
      })}
      {rest > 0 && <li className="ai-card__files-more">+{rest}</li>}
    </ul>
  );
}

interface AiCardProps {
  record: AiRecord;
  busy: boolean;
  readOnly: boolean;
  target: AiTaskTarget | null;
  onOpen: (record: AiRecord) => void;
  onToTask: (record: AiRecord, target: AiTaskTarget) => void;
  onAck: (record: AiRecord) => void;
  onRemove: (record: AiRecord) => void;
}

function AiCard({ record, busy, readOnly, target, onOpen, onToTask, onAck, onRemove }: AiCardProps) {
  const state = displayState(record);
  const title = record.title ?? '未命名会话';
  const outcome = record.outcome && record.outcome !== 'completed' ? AI_OUTCOME_LABEL[record.outcome] : null;
  const linked = record.task_id !== null;
  return (
    <article
      className={`ai-card ai-card--${state}${busy ? ' is-busy' : ''}`}
      data-testid="ai-card"
      data-session={record.session_id}
      aria-label={title}
    >
      <header className="ai-card__topline">
        <span className={`ai-badge ai-badge--${state}`}>{AI_STATE_LABEL[state]}</span>
        {outcome && <span className="ai-card__outcome">{outcome}</span>}
        <span className="ai-card__workspace" title={record.workspace}>{workspaceName(record.workspace)}</span>
        <time className="ai-card__time" dateTime={new Date(record.event_at).toISOString()} title={absoluteTime(record.event_at)}>
          {relativeTime(record.event_at)}
        </time>
      </header>
      <h3 className="ai-card__title">{title}</h3>
      {record.question && <QuestionPreview question={record.question} />}
      <ReplyPreview record={record} />
      <ChangedFiles record={record} />
      <footer className="ai-card__actions">
        <button
          type="button"
          className="text-action text-action--primary"
          disabled={busy || record.open_url === null}
          title={record.open_url ?? '来源没有提供会话链接'}
          onClick={() => onOpen(record)}
        >
          打开会话 <Icon name="arrow" />
        </button>
        {linked ? (
          <span className="ai-card__linked">已转为任务 <code>{shortId(record.task_id)}</code></span>
        ) : (
          <button
            type="button"
            className="text-action"
            disabled={busy || readOnly || target === null}
            title={target === null ? '先打开一个项目' : readOnly ? '只读模式下不能新增任务' : '在当前项目新增 Project Task'}
            onClick={() => target && onToTask(record, target)}
          >
            转为任务
          </button>
        )}
        {canAck(record) && (
          <button type="button" className="text-action" disabled={busy || readOnly} onClick={() => onAck(record)}>
            标记已读
          </button>
        )}
        <button
          type="button"
          className="icon-action ai-card__remove"
          disabled={busy || readOnly}
          aria-label={`移除会话：${title}`}
          title="从列表移除（不影响宿主）"
          onClick={() => onRemove(record)}
        >
          <Icon name="trash" />
        </button>
      </footer>
    </article>
  );
}

export function AiView({ feed, snapshot, newTaskSource, readOnly }: AiViewProps) {
  const counts = React.useMemo(() => countByState(feed.records), [feed.records]);
  const sections = React.useMemo(() => groupSections(feed.records, feed.visible), [feed.records, feed.visible]);
  const target = React.useMemo(() => taskTarget(snapshot, newTaskSource), [snapshot, newTaskSource]);

  const empty = (() => {
    if (feed.loading) return null;
    if (!feed.connected) return 'not-connected' as const;
    if (feed.records.length === 0) return feed.totalVisible > 0 && feed.scope === 'project' ? ('none-here' as const) : ('none' as const);
    if (sections.length === 0) return 'filtered' as const;
    return null;
  })();

  return (
    <section className="ai-view" aria-labelledby="ai-title">
      <header className="view-heading view-heading--ai">
        <div className="view-heading__title">
          <h1 id="ai-title">AI</h1>
          <p className="view-heading__meta">
            <span>{feed.records.length} 个会话</span>
            <span aria-hidden="true">·</span>
            <span>{feed.attention} 项需要关注</span>
            {readOnly && (
              <>
                <span aria-hidden="true">·</span>
                <span>只读浏览</span>
              </>
            )}
          </p>
        </div>
        <p className="view-heading__note">来自 agent 宿主的实时快照。它们不是 Project Task；标记与移除只记在本机。</p>
      </header>

      <div className="ai-toolbar">
        <div className="due-summary ai-pills" role="group" aria-label="按状态筛选会话">
          {AI_PILL_ORDER.map((state: AiState) => (
            <button
              type="button"
              key={state}
              className={`due-summary__chip ai-pill ai-pill--${state}${feed.visible.has(state) ? ' is-active' : ''}`}
              aria-pressed={feed.visible.has(state)}
              onClick={() => feed.toggleVisible(state)}
            >
              <span>{AI_STATE_LABEL[state]}</span>
              <strong>{counts[state]}</strong>
            </button>
          ))}
        </div>
        {feed.canScope && (
          <div className="ai-scope" role="group" aria-label="会话范围">
            <button type="button" className={feed.scope === 'project' ? 'is-active' : ''} aria-pressed={feed.scope === 'project'} onClick={() => feed.setScope('project')}>当前项目</button>
            <button type="button" className={feed.scope === 'all' ? 'is-active' : ''} aria-pressed={feed.scope === 'all'} onClick={() => feed.setScope('all')}>所有工作区</button>
          </div>
        )}
      </div>

      {feed.error && (
        <div className="ai-notice" role="alert">
          <Icon name="alert" />
          <span>{feed.error}</span>
          <button type="button" className="icon-action" aria-label="关闭提示" onClick={feed.clearError}><Icon name="close" /></button>
        </div>
      )}

      {feed.loading ? (
        <div className="capture-loading ai-loading" aria-label="正在读取会话">
          <span />
          <span />
        </div>
      ) : empty === 'not-connected' ? (
        <div className="ai-empty" data-testid="ai-empty-not-connected">
          <span className="capture-empty__circle"><Icon name="command" /></span>
          <h2>还没有宿主接入</h2>
          <p>在 DeepSeek Harness 侧启用桥接后，会话状态会经 <code>todo ai upsert</code> 写到这里。安装说明：</p>
          <pre><code>todo hooks instructions --agent deepseek-harness</code></pre>
          <p className="ai-empty__note">字段与状态见 <code>docs/ai-feed-spec.md</code>。</p>
        </div>
      ) : empty === 'none-here' ? (
        <div className="ai-empty" data-testid="ai-empty-project">
          <span className="capture-empty__circle"><Icon name="check" /></span>
          <h2>当前项目没有会话</h2>
          <p>宿主已接入，但没有工作目录位于这个项目之下的会话。切到「所有工作区」可以看到其余 {feed.totalVisible} 个。</p>
        </div>
      ) : empty === 'none' ? (
        <div className="ai-empty" data-testid="ai-empty-none">
          <span className="capture-empty__circle"><Icon name="check" /></span>
          <h2>暂时没有会话</h2>
          <p>宿主已接入。agent 开始工作后，会话会按状态出现在这里。</p>
        </div>
      ) : empty === 'filtered' ? (
        <div className="ai-empty" data-testid="ai-empty-filtered">
          <span className="capture-empty__circle"><Icon name="grid" /></span>
          <h2>当前筛选下没有会话</h2>
          <p>点亮上方的状态胶囊，或切换范围。</p>
        </div>
      ) : (
        <div className="ai-sections">
          {sections.map((section) => (
            <section className={`ai-section ai-section--${section.state}`} key={section.state} aria-labelledby={`ai-section-${section.state}`}>
              <header className="ai-section__header">
                <h2 id={`ai-section-${section.state}`}>{AI_STATE_LABEL[section.state]}</h2>
                <span className="goal-region__count">{section.records.length}</span>
              </header>
              <div className="ai-section__cards">
                {section.records.map((record) => (
                  <AiCard
                    key={recordKey(record)}
                    record={record}
                    busy={feed.busy.has(recordKey(record))}
                    readOnly={readOnly}
                    target={target}
                    onOpen={(item) => void feed.open(item)}
                    onToTask={(item, where) => void feed.toTask(item, where)}
                    onAck={(item) => void feed.ack(item)}
                    onRemove={(item) => void feed.remove(item)}
                  />
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </section>
  );
}
