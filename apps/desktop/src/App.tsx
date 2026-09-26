import * as React from 'react';
import { AiView } from './components/AiView';
import { CaptureView } from './components/CaptureView';
import { CommandBar } from './components/CommandBar';
import { ColumnSetupDialog } from './components/ColumnSetupDialog';
import { CompletionDialog, type CompletionDecision } from './components/CompletionDialog';
import { ConfirmCompleteDialog } from './components/ConfirmCompleteDialog';
import { EmptyState } from './components/EmptyState';
import { GoalView } from './components/GoalView';
import { PriorityScopeDialog } from './components/PriorityScopeDialog';
import { ProjectPickerDialog } from './components/ProjectPickerDialog';
import { QuadrantView } from './components/QuadrantView';
import { ProviderSettings } from './components/ProviderSettings';
import { ReviewView } from './components/ReviewView';
import { SettingsDialog } from './components/SettingsDialog';
import { Sidebar, type MainView } from './components/Sidebar';
import { StatusBanner } from './components/StatusBanner';
import { useAiFeed } from './hooks/useAiFeed';
import { useDesktop } from './hooks/useDesktop';
import { useAppSettings } from './lib/appSettings';
import { desktop, transportCapabilities, type DesktopTask, type TaskPriority } from './lib/desktop';
import { quadrantGroups } from './lib/quadrant';
import { SourceControls, SourcePreview } from './components/SourceControls';
import { SourceEnrollmentDialog } from './components/SourceEnrollmentDialog';
import { BatchAdoptDialog } from './components/BatchAdoptDialog';
import { commitSourceEnrollment, filterSnapshotBySources, useSourceEnrollment } from './lib/sourceEnrollment';

interface PriorityRequest {
  task: DesktopTask;
  priority: TaskPriority;
  descendantCount: number;
}

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
  const [projectPickerOpen, setProjectPickerOpen] = React.useState(false);
  const [providerSettingsOpen, setProviderSettingsOpen] = React.useState(false);
  const [completionRequest, setCompletionRequest] = React.useState<CompletionRequest | null>(null);
  const completionRequestRef = React.useRef<CompletionRequest | null>(null);
  const [priorityRequest, setPriorityRequest] = React.useState<PriorityRequest | null>(null);
  const [confirmTask, setConfirmTask] = React.useState<DesktopTask | null>(null);
  const [columnSetup, setColumnSetup] = React.useState<{
    projectPath: string;
    sourcePath: string;
    contextKey: string;
    restoreFocus: HTMLElement | null;
    startTask?: { localId: string; body: string };
  } | null>(null);
  const [appSettings] = useAppSettings();
  const model = useDesktop();
  const serverReadOnly = transportCapabilities()?.writable === false;
  const readOnly = serverReadOnly || model.sourceBusy;

  // Discovery reports every TODO.md under the root; the board shows the ones
  // the person enrolled. The full snapshot stays with the model (writes,
  // adoption and the enrollment sheet need it); views read the filtered one.
  const enrollment = useSourceEnrollment(model.snapshot);
  const [enrollmentOpen, setEnrollmentOpen] = React.useState(false);
  const [batchAdoptOpen, setBatchAdoptOpen] = React.useState(false);
  const snapshot = React.useMemo(
    () => (model.snapshot ? filterSnapshotBySources(model.snapshot, enrollment.enrolled) : null),
    [model.snapshot, enrollment.enrolled],
  );
  const discovered = model.snapshot?.sources ?? [];
  const showEnrollment = Boolean(model.snapshot?.sources && (enrollmentOpen || enrollment.needsDecision));
  React.useEffect(() => {
    // A remembered selection that is no longer enrolled falls back to the
    // aggregate view rather than showing an empty board for a hidden file.
    const selectedPath = model.snapshot?.selected_source;
    if (enrollment.enrolled && selectedPath && !enrollment.enrolled.has(selectedPath)) void model.selectSource(null);
    if (enrollment.enrolled && model.newTaskSource && !enrollment.enrolled.has(model.newTaskSource)) model.setNewTaskSource(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enrollment.enrolled, model.snapshot?.selected_source, model.newTaskSource]);

  const setLoose = React.useCallback((path: string, loose: boolean) => {
    const current = model.looseSources;
    const next = loose ? [...current.filter((item) => item !== path), path] : current.filter((item) => item !== path);
    void model.setLooseSources(next).then(() => {
      // A loose file has read-only tasks; the preview page is where they are
      // explained and where adoption starts, so land there when turning it on.
      if (loose && model.snapshot?.selected_source !== path) void model.selectSource(path);
    });
  }, [model]);
  const selected = snapshot?.sources?.find((source) => source.path === snapshot?.selected_source);
  const sourceContextKey = JSON.stringify([snapshot?.project_path, snapshot?.selected_source]);
  const sourceBoundUnavailable = Boolean(snapshot?.sources && (!selected || selected.status !== 'managed'));
  React.useEffect(() => {
    completionRequestRef.current?.settleMove?.(false);
    completionRequestRef.current = null;
    setCompletionRequest(null);
    setConfirmTask(null);
    setPriorityRequest(null);
    setColumnSetup(null);
  }, [sourceContextKey, model.sourceBusy]);
  const configureColumns = (task: DesktopTask, restoreFocus: HTMLElement | null, start = false) => {
    const current = model.snapshot;
    const latest = current?.tasks.find((item) => item.id === task.id && item.source_path === task.source_path);
    const source = current?.sources?.find((item) => item.path === task.source_path);
    if (!current || columnSetup || !latest || readOnly || latest.source_writable === false || !source?.writable || source.status !== 'managed'
      || current.columns.some((column) => column.source_path === source.path)) return;
    if (start && (!latest.local_id || latest.column !== null || latest.completed)) return;
    setColumnSetup({ projectPath: current.project_path, sourcePath: source.path, contextKey: sourceContextKey, restoreFocus,
      ...(start ? { startTask: { localId: latest.local_id!, body: latest.body } } : {}) });
  };
  const chooseProject = () => {
    if (desktop.isTauri) void model.chooseProject();
    else setProjectPickerOpen(true);
  };
  // The badge counts what needs attention today: overdue plus due-today work.
  const unrankedCount = snapshot
    ? quadrantGroups(snapshot).filter((group) => group.task.priority === null).length
    : 0;
  const goalCount = snapshot
    ? snapshot.today.overdue.length + snapshot.today.today.length
    : 0;
  // The AI feed is global and read independently of the project; the scope
  // toggle inside the view narrows it to sessions under the open root.
  const aiFeed = useAiFeed({ projectPath: model.snapshot?.project_path ?? null, onSnapshot: model.applySnapshot });

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
    if (!task || task.source_writable === false) return Promise.resolve(false);
    const policy = model.snapshot?.sources?.find((source) => source.path === task.source_path)?.completion_policy ?? model.snapshot?.completion_policy;
    if (policy === 'strict') {
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
    if (!task || !destination || task.source_writable === false) return Promise.resolve(false);
    if (task.source_path !== destination.source_path) return model.moveTask(id, column);
    const enteringDone = destination.done && !task.completed;
    const policy = model.snapshot?.sources?.find((source) => source.path === task.source_path)?.completion_policy ?? model.snapshot?.completion_policy;
    const needsOutcome = enteringDone && (
      policy === 'strict'
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

  const requestPriority = React.useCallback((task: DesktopTask, priority: TaskPriority) => {
    if (!task.id || !model.snapshot) return;
    const root = model.snapshot.tasks.findIndex((item) => item.id === task.id);
    let descendants = 0;
    for (let index = root + 1; index < model.snapshot.tasks.length; index += 1) {
      if (model.snapshot.tasks[index].column !== task.column || model.snapshot.tasks[index].indent_level <= task.indent_level) break;
      descendants += 1;
    }
    if (descendants === 0) void model.setPriority(task.id, priority);
    else setPriorityRequest({ task, priority, descendantCount: descendants });
  }, [model]);

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
    <div className="app-shell" data-density={appSettings.density}>
      <Sidebar
        activeView={activeView}
        status={model.status}
        goalCount={goalCount}
        captureCount={model.captures.length}
        unrankedCount={unrankedCount}
        aiCount={aiFeed.attention}
        readOnly={readOnly}
        onChangeView={setActiveView}
        onChooseProject={chooseProject}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <div className={`workspace${readOnly ? ' is-read-only' : ''}`}>
        <CommandBar
          snapshot={snapshot}
          projectBusy={model.busyKeys.has('project')}
          addBusy={model.busyKeys.has('task:new')}
          readOnly={readOnly}
          onChooseProject={chooseProject}
          onRefresh={() => void model.refreshSnapshot()}
          onOpenProviderSettings={() => setProviderSettingsOpen(true)}
          onAddTask={model.addTask}
          newTaskSource={model.newTaskSource}
          onNewTaskSource={(path) => {
            model.setNewTaskSource(path);
            if (path && snapshot?.selected_source && path !== snapshot.selected_source) void model.selectSource(path);
          }}
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

        <main className={`workspace__content${snapshot?.sources ? ' has-sources' : ''}`}>
          {snapshot?.sources && (
            <SourceControls
              snapshot={snapshot}
              busy={model.sourceBusy || model.busyKeys.has('project')}
              discoveredCount={discovered.length}
              pendingCount={enrollment.pending.length}
              onSelect={(path) => void model.selectSource(path)}
              onAdjustEnrollment={() => setEnrollmentOpen(true)}
              onBatchAdopt={readOnly ? undefined : () => setBatchAdoptOpen(true)}
            />
          )}
          <div className="source-content" key={sourceContextKey}>
          {activeView === 'capture' && transportCapabilities() ? (
            <p className="source-unavailable" role="status">HTTP 项目服务不开放个人随记，请使用桌面版。不会显示或写入模拟随记。</p>
          ) : activeView === 'capture' ? (
            <CaptureView
              snapshot={snapshot}
              captures={model.captures}
              capturesLoading={model.capturesLoading}
              busyKeys={model.busyKeys}
              partialTriages={model.partialTriages}
              readOnly={readOnly}
              onAppend={model.appendCapture}
              onDelete={model.deleteCapture}
              onTriage={model.triageCapture}
              triageUnavailable={sourceBoundUnavailable || Boolean(selected && !selected.writable)}
            />
          ) : activeView === 'ai' ? (
            <AiView
              feed={aiFeed}
              snapshot={snapshot}
              newTaskSource={model.newTaskSource}
              readOnly={serverReadOnly || model.sourceBusy}
            />
          ) : activeView === 'review' ? (
            sourceBoundUnavailable ? <p className="source-unavailable">扫描、历史与对齐提示词依赖单个来源。请先在「任务来源」选择已管理文件。</p> :
            <ReviewView snapshot={snapshot} readOnly={readOnly || Boolean(selected && !selected.writable)} onApplied={model.applySnapshot} />
          ) : selected && selected.status !== 'managed' ? (
            <SourcePreview key={`${sourceContextKey}:${selected.revision}:${selected.loose ? 'loose' : ''}`} source={selected} readOnly={serverReadOnly || model.sourceBusy} onAdopt={model.adoptSource} onSetLoose={setLoose} />
          ) : activeView === 'quadrant' && snapshot ? (
            <QuadrantView
              snapshot={snapshot}
              busyKeys={model.busyKeys}
              readOnly={readOnly}
              onSetCompleted={setCompleted}
              onSetPartial={model.setPartial}
              onSetFailed={model.setFailed}
              onMove={(id, column) => void moveTask(id, column)}
              onSetPriority={(id, priority) => model.setPriority(id, priority, 'subtree')}
              onConfigureColumns={configureColumns}
            />
          ) : snapshot ? (
            <GoalView
              snapshot={snapshot}
              busyKeys={model.busyKeys}
              readOnly={readOnly}
              onSetCompleted={setCompleted}
              onSetPartial={model.setPartial}
              onSetFailed={model.setFailed}
              onMove={moveTask}
              onSetPriority={(id, priority) => { const task = model.snapshot?.tasks.find((item) => item.id === id); if (task) requestPriority(task, priority); }}
              onSetLoose={readOnly ? undefined : setLoose}
              onConfigureColumns={configureColumns}
            />
          ) : (
            <EmptyState
              phase={model.phase === 'ready' ? 'load-error' : model.phase}
              onChooseProject={chooseProject}
              onRetry={() => void model.refreshSnapshot()}
            />
          )}
          </div>
        </main>
      </div>

      {columnSetup && columnSetup.contextKey === sourceContextKey && !model.sourceBusy && (
        <ColumnSetupDialog
          key={JSON.stringify([columnSetup.projectPath, columnSetup.sourcePath, columnSetup.startTask?.localId])}
          projectPath={columnSetup.projectPath}
          sourcePath={columnSetup.sourcePath}
          restoreFocus={columnSetup.restoreFocus}
          startTask={columnSetup.startTask}
          readOnly={serverReadOnly || !model.snapshot?.sources?.some((source) => source.path === columnSetup.sourcePath && source.status === 'managed' && source.writable)
            || model.snapshot.tasks.some((task) => task.source_path === columnSetup.sourcePath && task.source_writable === false)}
          onApply={model.applyColumns}
          onClose={() => setColumnSetup(null)}
        />
      )}
      {projectPickerOpen && <ProjectPickerDialog onClose={() => setProjectPickerOpen(false)} />}
      {showEnrollment && model.snapshot && (
        <SourceEnrollmentDialog
          key={model.snapshot.project_path}
          projectPath={model.snapshot.project_path}
          sources={discovered}
          enrolled={enrollment.enrolled}
          pending={new Set(enrollment.pending.map((source) => source.path))}
          firstRun={enrollment.needsDecision}
          onCancel={() => {
            // Dismissing the first-run sheet is the same as 全部录入: the board
            // stays complete and 「调整录入…」 remains one click away.
            if (enrollment.needsDecision) commitSourceEnrollment(model.snapshot!.project_path, discovered.map((source) => source.path), []);
            setEnrollmentOpen(false);
          }}
          onConfirm={(paths) => {
            commitSourceEnrollment(model.snapshot!.project_path, paths, discovered.map((source) => source.path));
            setEnrollmentOpen(false);
          }}
        />
      )}
      {batchAdoptOpen && snapshot?.sources && (
        <BatchAdoptDialog
          sources={snapshot.sources.filter((source) => source.status === 'unmanaged')}
          readOnly={readOnly}
          onAdopt={model.adoptSources}
          onClose={() => setBatchAdoptOpen(false)}
        />
      )}
      {priorityRequest && (
        <PriorityScopeDialog
          task={priorityRequest.task}
          descendantCount={priorityRequest.descendantCount}
          priority={priorityRequest.priority}
          busy={Boolean(priorityRequest.task.id && model.busyKeys.has(`task:${priorityRequest.task.id}`))}
          onCancel={() => setPriorityRequest(null)}
          onConfirm={async (scope) => {
            if (!priorityRequest.task.id) return;
            const written = await model.setPriority(priorityRequest.task.id, priorityRequest.priority, scope);
            if (written) setPriorityRequest(null);
          }}
        />
      )}
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
