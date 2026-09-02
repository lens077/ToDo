import * as React from 'react';
import type { DesktopTask, EvidenceRange } from '../lib/desktop';

export type CompletionDecision =
  | { outcome: 'done'; note: string; evidence?: EvidenceRange }
  | { outcome: 'partial'; percent: number; note: string }
  | { outcome: 'failed'; note: string };

type Outcome = CompletionDecision['outcome'];

interface CompletionDialogProps {
  task: DesktopTask;
  initialOutcome?: Outcome;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (decision: CompletionDecision) => void;
}

const outcomes: Array<{ value: Outcome; label: string; detail: string }> = [
  { value: 'done', label: '完成', detail: '验收通过，关闭任务' },
  { value: 'partial', label: '部分完成', detail: '保留任务，记录当前进度' },
  { value: 'failed', label: '失败', detail: '保留任务，记录失败原因' },
];

export function CompletionDialog({ task, initialOutcome = 'done', busy, onCancel, onConfirm }: CompletionDialogProps) {
  const [outcome, setOutcome] = React.useState<Outcome>(initialOutcome);
  const [percent, setPercent] = React.useState(task.progress ?? 50);
  // Always start empty: the explanation must be written for this outcome, so a
  // stale note can never be re-submitted as if it were the current one.
  const [note, setNote] = React.useState('');
  const availability = task.evidence_availability;
  const availableRange: EvidenceRange | undefined = availability.status === 'available'
    ? { from: availability.from, to: availability.to }
    : undefined;
  const unavailableReason = availability.status === 'unavailable' ? availability.reason : '';
  const [includeEvidence, setIncludeEvidence] = React.useState(availableRange !== undefined);
  const trimmedNote = note.trim();
  const canSubmit = Boolean(trimmedNote) && (outcome !== 'partial' || (percent >= 1 && percent <= 99)) && !busy;

  React.useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) onCancel();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [busy, onCancel]);

  const noteLabel = outcome === 'done' ? '完成说明' : outcome === 'partial' ? '进度说明' : '失败原因';

  return (
    <div className="completion-dialog" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget && !busy) onCancel();
    }}>
      <section className="completion-panel" role="dialog" aria-modal="true" aria-labelledby="completion-dialog-title">
        <header className="completion-panel__header">
          <span className="eyebrow">TASK OUTCOME</span>
          <h2 id="completion-dialog-title">确认任务结局</h2>
          <p>{task.body}</p>
        </header>
        <form className="completion-form" onSubmit={(event) => {
          event.preventDefault();
          if (!canSubmit) return;
          if (outcome === 'done') onConfirm({ outcome, note: trimmedNote, evidence: includeEvidence ? availableRange : undefined });
          else if (outcome === 'partial') onConfirm({ outcome, percent, note: trimmedNote });
          else onConfirm({ outcome, note: trimmedNote });
        }}>
          <fieldset className="completion-outcomes">
            <legend>选择结局</legend>
            {outcomes.map((option) => (
              <label key={option.value} className={outcome === option.value ? 'is-selected' : undefined}>
                <input type="radio" name="completion-outcome" checked={outcome === option.value} onChange={() => setOutcome(option.value)} />
                <strong>{option.label}</strong><span>{option.detail}</span>
              </label>
            ))}
          </fieldset>
          {outcome === 'partial' && (
            <label className="completion-field">
              <span>完成比例 <b>1–99</b></span>
              <input aria-label="完成比例" type="number" min={1} max={99} value={percent} onChange={(event) => setPercent(Number(event.target.value))} />
            </label>
          )}
          <label className="completion-field">
            <span>{noteLabel} <b>必填</b></span>
            <textarea aria-label={noteLabel} autoFocus rows={4} value={note} onChange={(event) => setNote(event.target.value)} />
            <small>本次结局必须留下说明；旧说明不会自动沿用。</small>
          </label>
          <label className={`completion-evidence completion-evidence--${availableRange ? 'available' : 'unavailable'}`}>
            <input type="checkbox" checked={includeEvidence && outcome === 'done'} disabled={!availableRange || outcome !== 'done'} onChange={(event) => setIncludeEvidence(event.target.checked)} />
            <span className="completion-evidence__copy">
              <strong>{availableRange ? '附带 Git 物证' : '物证不可用'}</strong>
              {availableRange ? <code>{availableRange.from}..{availableRange.to}</code> : <small>{unavailableReason}</small>}
            </span>
          </label>
          <footer className="completion-panel__actions">
            <button type="button" className="text-action" onClick={onCancel} disabled={busy}>取消</button>
            <button type="submit" className="text-action text-action--primary" disabled={!canSubmit}>
              {busy ? '写入中…' : `确认${outcomes.find((item) => item.value === outcome)?.label}`}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
