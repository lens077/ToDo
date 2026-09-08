import * as React from 'react';
import { desktop, type AdoptPreview, type TaskSource } from '../lib/desktop';
import { DiffView } from './DiffView';
import { Icon } from './Icon';

interface BatchAdoptDialogProps {
  /** Every discovered source that is still unmanaged. */
  sources: TaskSource[];
  readOnly: boolean;
  onAdopt: (
    items: Array<{ path: string; baseHash: string }>,
    onProgress: (path: string, outcome: 'adopted' | 'failed', message?: string) => void,
  ) => Promise<{ adopted: string[]; failed: { path: string; message: string } | null }>;
  onClose: () => void;
}

type Phase = 'select' | 'previewing' | 'review' | 'applying' | 'done';

interface PreviewState {
  preview?: AdoptPreview;
  error?: string;
  outcome?: 'adopted' | 'failed';
  message?: string;
}

/**
 * Adopts several unmanaged sources in one sitting.
 *
 * The rule from single adoption holds for every file: the person sees the
 * exact bytes that will be inserted before anything is written, and each
 * write is locked to the hash they reviewed. Batch only removes the walking
 * between files, not the review.
 */
export function BatchAdoptDialog({ sources, readOnly, onAdopt, onClose }: BatchAdoptDialogProps) {
  const [checked, setChecked] = React.useState<Set<string>>(() => new Set(sources.map((source) => source.path)));
  const [phase, setPhase] = React.useState<Phase>('select');
  const [states, setStates] = React.useState<Record<string, PreviewState>>({});
  const [expanded, setExpanded] = React.useState<string | null>(null);
  const alive = React.useRef(true);
  React.useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && phase !== 'previewing' && phase !== 'applying') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose, phase]);

  const selected = sources.filter((source) => checked.has(source.path));
  const previewed = selected.filter((source) => states[source.path]?.preview);
  const previewErrors = selected.filter((source) => states[source.path]?.error);
  const totalAdded = previewed.reduce((sum, source) => sum + (states[source.path].preview?.diff.added ?? 0), 0);
  const adoptedCount = Object.values(states).filter((state) => state.outcome === 'adopted').length;
  const failure = Object.entries(states).find(([, state]) => state.outcome === 'failed');

  const toggle = (path: string, on: boolean) => setChecked((current) => {
    const next = new Set(current);
    if (on) next.add(path); else next.delete(path);
    return next;
  });

  const previewAll = async () => {
    if (selected.length === 0 || readOnly) return;
    setPhase('previewing');
    const results = await Promise.all(selected.map(async (source) => {
      try {
        const preview = await desktop.previewAdopt(source.path);
        if (preview.source_path !== source.path) throw new Error('预览来源不一致，未写入文件。');
        return [source.path, { preview }] as const;
      } catch (cause) {
        return [source.path, { error: cause instanceof Error ? cause.message : String(cause) }] as const;
      }
    }));
    if (!alive.current) return;
    setStates(Object.fromEntries(results));
    setPhase('review');
  };

  const applyAll = async () => {
    if (previewed.length === 0 || readOnly) return;
    setPhase('applying');
    await onAdopt(
      previewed.map((source) => ({ path: source.path, baseHash: states[source.path].preview!.base_hash })),
      (path, outcome, message) => {
        if (!alive.current) return;
        setStates((current) => ({ ...current, [path]: { ...current[path], outcome, message } }));
      },
    );
    if (alive.current) setPhase('done');
  };

  const busy = phase === 'previewing' || phase === 'applying';

  return (
    <div
      className="provider-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <section className="enroll-panel batch-adopt" role="dialog" aria-modal="true" aria-labelledby="batch-adopt-title" aria-describedby="batch-adopt-lede">
        <header className="enroll-panel__header">
          <div>
            <h2 id="batch-adopt-title">批量接管来源</h2>
            <p id="batch-adopt-lede">
              {phase === 'select' || phase === 'previewing'
                ? `${sources.length} 个来源尚未接管。接管只在每个复选框行尾追加稳定 ID，并在需要时加上 todo-spec 声明；正文、换行与其他内容逐字节保留。先生成每个文件的差异，确认后再写入。`
                : phase === 'review'
                  ? `已生成 ${previewed.length} 个文件的差异，共新增 ${totalAdded} 行。${previewErrors.length ? `${previewErrors.length} 个文件无法预览，将被跳过。` : ''}文件若在确认前被外部改动，服务会拒绝写入并停止。`
                  : phase === 'applying'
                    ? '正在按顺序写入，每个文件都以预览时的指纹为准…'
                    : failure
                      ? `已接管 ${adoptedCount} 个文件；在「${failure[0]}」停止：${failure[1].message} 未接管的文件保持原样，重新预览后可再试。`
                      : `已接管 ${adoptedCount} 个文件，全部可写。`}
            </p>
          </div>
          {!busy && (
            <button type="button" className="icon-action" aria-label="关闭" onClick={onClose}>
              <Icon name="close" />
            </button>
          )}
        </header>

        {(phase === 'select' || phase === 'previewing') && (
          <div className="enroll-toolbar" role="group" aria-label="批量选择">
            <button type="button" className="enroll-toolbar__action" disabled={busy} onClick={() => setChecked(new Set(sources.map((source) => source.path)))}>全选</button>
            <button type="button" className="enroll-toolbar__action" disabled={busy || checked.size === 0} onClick={() => setChecked(new Set())}>清空</button>
            <span className="enroll-toolbar__count" aria-live="polite">已选 {checked.size} / {sources.length}</span>
          </div>
        )}

        <div className="enroll-list" role="group" aria-label="待接管的来源">
          {(phase === 'select' || phase === 'previewing' ? sources : selected).map((source) => {
            const state = states[source.path];
            const on = checked.has(source.path);
            const [directory, file] = splitPath(source.path);
            const summary = state?.outcome === 'adopted'
              ? <span className="enroll-row__status enroll-row__status--managed">已接管</span>
              : state?.outcome === 'failed'
                ? <span className="enroll-row__status enroll-row__status--error">失败</span>
                : state?.error
                  ? <span className="enroll-row__status enroll-row__status--error">无法预览</span>
                  : state?.preview
                    ? <span className="enroll-row__status">+{state.preview.diff.added} 行</span>
                    : phase === 'previewing' && on
                      ? <span className="enroll-row__status">生成中…</span>
                      : null;
            const row = (
              <>
                <span className="enroll-row__copy">
                  <code className="enroll-row__path">{directory && <span>{directory}</span>}{file}</code>
                  {(state?.error || state?.message) && <small className="enroll-row__error">{state.error ?? state.message}</small>}
                </span>
                {summary}
                <span className="enroll-row__count">{source.task_count} 项</span>
              </>
            );
            if (phase === 'select' || phase === 'previewing') {
              return (
                <label key={source.path} className={`enroll-row${on ? ' is-on' : ''}`}>
                  <input type="checkbox" checked={on} disabled={busy} onChange={(event) => toggle(source.path, event.target.checked)} aria-label={`接管 ${source.path}`} />
                  <span className="enroll-row__check" aria-hidden="true">{on && <Icon name="check" />}</span>
                  {row}
                </label>
              );
            }
            const open = expanded === source.path;
            return (
              <div key={source.path} className="batch-adopt__item">
                <button
                  type="button"
                  className={`enroll-row batch-adopt__row${open ? ' is-open' : ''}`}
                  aria-expanded={state?.preview ? open : undefined}
                  disabled={!state?.preview}
                  onClick={() => setExpanded(open ? null : source.path)}
                >
                  <Icon name="chevron" className="batch-adopt__chevron" />
                  {row}
                </button>
                {open && state?.preview && (
                  <div className="batch-adopt__diff">
                    <DiffView diff={state.preview.diff} leftLabel={source.path} rightLabel="接管后" />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <footer className="enroll-panel__actions">
          {phase === 'select' || phase === 'previewing' ? (
            <>
              <button type="button" className="text-action" disabled={busy} onClick={onClose}>取消</button>
              <button type="button" className="solid-action" disabled={busy || readOnly || selected.length === 0} onClick={() => void previewAll()}>
                {phase === 'previewing' ? '生成差异…' : `预览差异（${selected.length}）`}
              </button>
            </>
          ) : phase === 'review' || phase === 'applying' ? (
            <>
              <button type="button" className="text-action" disabled={busy} onClick={() => { setStates({}); setExpanded(null); setPhase('select'); }}>返回选择</button>
              <button type="button" className="solid-action" disabled={busy || readOnly || previewed.length === 0} onClick={() => void applyAll()}>
                {phase === 'applying' ? '写入中…' : `确认接管并写入（${previewed.length}）`}
              </button>
            </>
          ) : (
            <button type="button" className="solid-action" onClick={onClose}>完成</button>
          )}
        </footer>
      </section>
    </div>
  );
}

function splitPath(path: string): [string, string] {
  const index = path.lastIndexOf('/');
  return index === -1 ? ['', path] : [path.slice(0, index + 1), path.slice(index + 1)];
}
