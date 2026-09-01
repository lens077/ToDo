import * as React from 'react';
import { BoardView } from './components/BoardView';
import { CaptureView } from './components/CaptureView';
import { CommandBar } from './components/CommandBar';
import { CompletionDialog, type CompletionDecision } from './components/CompletionDialog';
import { EmptyState } from './components/EmptyState';
import { ProviderSettings } from './components/ProviderSettings';
import { ReviewView } from './components/ReviewView';
import { Sidebar, type MainView } from './components/Sidebar';
import { StatusBanner } from './components/StatusBanner';
import { TodayView } from './components/TodayView';
import { useDesktop } from './hooks/useDesktop';
import { transportCapabilities, type DesktopTask } from './lib/desktop';

export function App() {
  // The homepage answers "what should I do now"; the board is one click away.
  const [activeView, setActiveView] = React.useState<MainView>('today');
  const [providerSettingsOpen, setProviderSettingsOpen] = React.useState(false);
  const [completionTask, setCompletionTask] = React.useState<DesktopTask | null>(null);
  const [completionOutcome, setCompletionOutcome] = React.useState<CompletionDecision['outcome']>('done');
  const model = useDesktop();
  const readOnly = transportCapabilities()?.writable === false;
  const todayCount = model.snapshot
    ? model.snapshot.today.overdue.length + model.snapshot.today.today.length + model.snapshot.today.upcoming.length
    : 0;

  const setCompleted = React.useCallback((id: string, completed: boolean) => {
    if (!completed || model.snapshot?.completion_policy !== 'strict') {
      return model.setCompleted(id, completed);
    }
    const task = model.snapshot.tasks.find((item) => item.id === id);
    if (!task) return Promise.resolve(false);
    setCompletionOutcome('done');
    setCompletionTask(task);
    return Promise.resolve(false);
  }, [model]);

  const recordOutcome = React.useCallback((id: string, outcome: 'partial' | 'failed') => {
    const task = model.snapshot?.tasks.find((item) => item.id === id);
    if (!task) return;
    setCompletionOutcome(outcome);
    setCompletionTask(task);
  }, [model.snapshot]);

  const confirmCompletion = React.useCallback(async (decision: CompletionDecision) => {
    if (!completionTask?.id) return;
    const written = decision.outcome === 'done'
      ? await model.setCompleted(completionTask.id, true, decision.note, decision.evidence)
      : decision.outcome === 'partial'
        ? await model.setPartial(completionTask.id, decision.percent, decision.note)
        : await model.setFailed(completionTask.id, decision.note);
    if (written) setCompletionTask(null);
  }, [completionTask, model]);

  return (
    <div className="app-shell">
      <Sidebar
        activeView={activeView}
        status={model.status}
        todayCount={todayCount}
        captureCount={model.captures.length}
        readOnly={readOnly}
        onChangeView={setActiveView}
        onChooseProject={() => void model.chooseProject()}
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
            activeView === 'board' ? (
              <BoardView
                snapshot={model.snapshot}
                busyKeys={model.busyKeys}
                readOnly={readOnly}
                onSetCompleted={setCompleted}
                onRecordOutcome={recordOutcome}
                onMove={model.moveTask}
              />
            ) : (
              <TodayView
                snapshot={model.snapshot}
                busyKeys={model.busyKeys}
                readOnly={readOnly}
                onSetCompleted={setCompleted}
                onRecordOutcome={recordOutcome}
                onMove={model.moveTask}
              />
            )
          ) : (
            <EmptyState
              phase={model.phase === 'ready' ? 'load-error' : model.phase}
              onChooseProject={() => void model.chooseProject()}
              onRetry={() => void model.refreshSnapshot()}
            />
          )}
        </main>
      </div>

      {completionTask && (
        <CompletionDialog
          task={completionTask}
          initialOutcome={completionOutcome}
          busy={Boolean(completionTask.id && model.busyKeys.has(`task:${completionTask.id}`))}
          onCancel={() => setCompletionTask(null)}
          onConfirm={(decision) => void confirmCompletion(decision)}
        />
      )}
      {providerSettingsOpen && <ProviderSettings onClose={() => setProviderSettingsOpen(false)} />}
    </div>
  );
}
