import * as React from 'react';
import { CaptureView } from './components/CaptureView';
import { CommandBar } from './components/CommandBar';
import { CompletionDialog, type CompletionDecision } from './components/CompletionDialog';
import { ConfirmCompleteDialog } from './components/ConfirmCompleteDialog';
import { EmptyState } from './components/EmptyState';
import { GoalView } from './components/GoalView';
import { ProviderSettings } from './components/ProviderSettings';
import { ReviewView } from './components/ReviewView';
import { SettingsDialog } from './components/SettingsDialog';
import { Sidebar, type MainView } from './components/Sidebar';
import { StatusBanner } from './components/StatusBanner';
import { useDesktop } from './hooks/useDesktop';
import { useAppSettings } from './lib/appSettings';
import { transportCapabilities, type DesktopTask } from './lib/desktop';

interface CompletionRequest {
  task: DesktopTask;
  /** Present when the dialog was opened by a move into Done. */
  destinationColumn?: string;
  settleMove?: (moved: boolean) => void;
}

export function App() {
  // 目标 is the landing view: in-flight work, the queue, and finished work on
  // one surface, so "what should I do now" is answered without navigating.
  const [activeView, setActiveView] = React.useState<MainView>('goal');
  const [settingsOpen, setSettingsOpen] = React.useState(false);
  const [providerSettingsOpen, setProviderSettingsOpen] = React.useState(false);
  const [completionRequest, setCompletionRequest] = React.useState<CompletionRequest | null>(null);
  const completionRequestRef = React.useRef<CompletionRequest | null>(null);
  const [confirmTask, setConfirmTask] = React.useState<DesktopTask | null>(null);
  const [appSettings] = useAppSettings();
  const model = useDesktop();
  const readOnly = transportCapabilities()?.writable === false;
  // The badge counts what needs attention today: overdue plus due-today work.
  const goalCount = model.snapshot
    ? model.snapshot.today.overdue.length + model.snapshot.today.today.length
    : 0;

  const openCompletion = React.useCallback((request: CompletionRequest) => {
    completionRequestRef.current?.settleMove?.(false);
    completionRequestRef.current = request;
    setCompletionRequest(request);
  }, []);

  React.useEffect(() => () => {
    completionRequestRef.current?.settleMove?.(false);
    completionRequestRef.current = null;
  }, []);

  const setCompleted = React.useCallback((id: string, completed: boolean) => {
    if (!completed) return model.setCompleted(id, false);
    const task = model.snapshot?.tasks.find((item) => item.id === id);
    if (!task) return Promise.resolve(false);
    if (model.snapshot?.completion_policy === 'strict') {
      // Strict completion needs a note and an evidence decision; that is the
      // one outcome that still earns a full outcome dialog.
      openCompletion({ task });
      return Promise.resolve(false);
    }
    if (appSettings.confirmCompletion) {
      // A tick is a file write; the one-key gate keeps a stray click from
      // becoming one. Enter confirms, Escape backs out.
      setConfirmTask(task);
      return Promise.resolve(false);
    }
    return model.setCompleted(id, true);
  }, [appSettings.confirmCompletion, model, openCompletion]);

  const moveTask = React.useCallback((id: string, column: string): Promise<boolean> => {
    const task = model.snapshot?.tasks.find((item) => item.id === id);
    const destination = model.snapshot?.columns.find((item) => item.id === column);
    if (!task || !destination) return Promise.resolve(false);
    const enteringDone = destination.done && !task.completed;
    const needsOutcome = enteringDone && (
      model.snapshot?.completion_policy === 'strict'
      || task.progress !== null
      || task.failed_on !== null
    );
    if (!needsOutcome) return model.moveTask(id, column);

    // Keep the drop pending until the protected outcome write succeeds or the
    // user cancels. This lets the live announcement report the real result.
    return new Promise<boolean>((settleMove) => {
      openCompletion({ task, destinationColumn: column, settleMove });
    });
  }, [model, openCompletion]);

  const confirmTick = React.useCallback(async () => {
    if (!confirmTask?.id) return;
    const written = await model.setCompleted(confirmTask.id, true);
    if (written) setConfirmTask(null);
  }, [confirmTask, model]);

  const cancelCompletion = React.useCallback(() => {
    completionRequestRef.current?.settleMove?.(false);
    completionRequestRef.current = null;
    setCompletionRequest(null);
  }, []);

  const confirmCompletion = React.useCallback(async (decision: CompletionDecision) => {
    const taskId = completionRequest?.task.id;
    if (!taskId) return;
    const { destinationColumn, settleMove } = completionRequest;
    const written = decision.outcome === 'done'
      ? destinationColumn
        ? await model.moveTask(taskId, destinationColumn, {
            note: decision.note,
            evidence: decision.evidence,
          })
        : await model.setCompleted(taskId, true, decision.note, decision.evidence)
      : decision.outcome === 'partial'
        ? await model.setPartial(taskId, decision.percent, decision.note)
        : await model.setFailed(taskId, decision.note);
    if (written) {
      settleMove?.(decision.outcome === 'done' && Boolean(destinationColumn));
      if (completionRequestRef.current === completionRequest) {
        completionRequestRef.current = null;
        setCompletionRequest(null);
      }
    }
  }, [completionRequest, model]);

  return (
    <div className="app-shell">
      <Sidebar
        activeView={activeView}
        status={model.status}
        goalCount={goalCount}
        captureCount={model.captures.length}
        readOnly={readOnly}
        onChangeView={setActiveView}
        onChooseProject={() => void model.chooseProject()}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <div className={`workspace${readOnly ? ' is-read-only' : ''}`}>
        <CommandBar
          snapshot={model.snapshot}
          projectBusy={model.busyKeys.has('project')}
          addBusy={model.busyKeys.has('task:new')}
          readOnly={readOnly}
          onChooseProject={() => void model.chooseProject()}
          onRefresh={() => void model.refreshSnapshot()}
          onOpenProviderSettings={() => setProviderSettingsOpen(true)}
          onAddTask={model.addTask}
        />

        {readOnly && (
          <div className="read-only-notice" role="status" data-testid="read-only-notice">
            <span className="read-only-notice__mark" aria-hidden="true">RO</span>
            <strong>只读模式</strong>
            <span>此设备仅可查看；新增、完成、拖拽、导入与恢复均已停用。</span>
          </div>
        )}

        {model.banner && (
          <StatusBanner
            banner={model.banner}
            onDismiss={model.clearBanner}
            onRetry={() => void model.refreshSnapshot()}
          />
        )}

        <main className="workspace__content">
          {activeView === 'capture' ? (
            <CaptureView
              snapshot={model.snapshot}
              captures={model.captures}
              capturesLoading={model.capturesLoading}
              busyKeys={model.busyKeys}
              partialTriages={model.partialTriages}
              readOnly={readOnly}
              onAppend={model.appendCapture}
              onDelete={model.deleteCapture}
              onTriage={model.triageCapture}
            />
          ) : activeView === 'review' ? (
            <ReviewView snapshot={model.snapshot} readOnly={readOnly} onApplied={model.applySnapshot} />
          ) : model.snapshot ? (
            <GoalView
              snapshot={model.snapshot}
              busyKeys={model.busyKeys}
              readOnly={readOnly}
              onSetCompleted={setCompleted}
              onSetPartial={model.setPartial}
              onSetFailed={model.setFailed}
              onMove={moveTask}
            />
          ) : (
            <EmptyState
              phase={model.phase === 'ready' ? 'load-error' : model.phase}
              onChooseProject={() => void model.chooseProject()}
              onRetry={() => void model.refreshSnapshot()}
            />
          )}
        </main>
      </div>

      {completionRequest && (
        <CompletionDialog
          key={`${completionRequest.task.id ?? completionRequest.task.body}:${completionRequest.destinationColumn ?? 'checkbox'}`}
          task={completionRequest.task}
          initialOutcome="done"
          busy={Boolean(
            completionRequest.task.id
            && model.busyKeys.has(`task:${completionRequest.task.id}`)
          )}
          onCancel={cancelCompletion}
          onConfirm={(decision) => void confirmCompletion(decision)}
        />
      )}
      {confirmTask && (
        <ConfirmCompleteDialog
          task={confirmTask}
          busy={Boolean(confirmTask.id && model.busyKeys.has(`task:${confirmTask.id}`))}
          onCancel={() => setConfirmTask(null)}
          onConfirm={() => void confirmTick()}
        />
      )}
      {settingsOpen && (
        <SettingsDialog
          onClose={() => setSettingsOpen(false)}
          onOpenProviderSettings={() => {
            setSettingsOpen(false);
            setProviderSettingsOpen(true);
          }}
        />
      )}
      {providerSettingsOpen && <ProviderSettings onClose={() => setProviderSettingsOpen(false)} />}
    </div>
  );
}
