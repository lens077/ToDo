import * as React from 'react';
import { BoardView } from './components/BoardView';
import { CaptureView } from './components/CaptureView';
import { CommandBar } from './components/CommandBar';
import { EmptyState } from './components/EmptyState';
import { Sidebar, type MainView } from './components/Sidebar';
import { StatusBanner } from './components/StatusBanner';
import { TodayView } from './components/TodayView';
import { useDesktop } from './hooks/useDesktop';

export function App() {
  const [activeView, setActiveView] = React.useState<MainView>('board');
  const model = useDesktop();
  const todayCount = model.snapshot
    ? model.snapshot.today.overdue.length + model.snapshot.today.today.length + model.snapshot.today.upcoming.length
    : 0;

  return (
    <div className="app-shell">
      <Sidebar
        activeView={activeView}
        status={model.status}
        todayCount={todayCount}
        captureCount={model.captures.length}
        onChangeView={setActiveView}
        onChooseProject={() => void model.chooseProject()}
      />

      <div className="workspace">
        <CommandBar
          snapshot={model.snapshot}
          projectBusy={model.busyKeys.has('project')}
          addBusy={model.busyKeys.has('task:new')}
          onChooseProject={() => void model.chooseProject()}
          onRefresh={() => void model.refreshSnapshot()}
          onAddTask={model.addTask}
        />

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
              onAppend={model.appendCapture}
              onDelete={model.deleteCapture}
              onTriage={model.triageCapture}
            />
          ) : model.snapshot ? (
            activeView === 'board' ? (
              <BoardView
                snapshot={model.snapshot}
                busyKeys={model.busyKeys}
                onSetCompleted={model.setCompleted}
                onMove={model.moveTask}
              />
            ) : (
              <TodayView
                snapshot={model.snapshot}
                busyKeys={model.busyKeys}
                onSetCompleted={model.setCompleted}
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
    </div>
  );
}
