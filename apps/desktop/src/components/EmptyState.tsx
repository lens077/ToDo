import type { LoadPhase } from '../hooks/useDesktop';
import { Icon } from './Icon';
import { Ribbon } from './Ribbon';

interface EmptyStateProps {
  phase: Extract<LoadPhase, 'loading' | 'no-project' | 'load-error'>;
  onChooseProject: () => void;
  onRetry: () => void;
}

export function EmptyState({ phase, onChooseProject, onRetry }: EmptyStateProps) {
  if (phase === 'loading') {
    return (
      <section className="loading-state" aria-label="正在载入桌面工作台">
        <div className="loading-state__heading"><span /><span /></div>
        <div className="loading-state__columns">
          <span /><span /><span />
        </div>
      </section>
    );
  }

  const failed = phase === 'load-error';
  return (
    <section className={`project-empty${failed ? ' project-empty--error' : ''}`}>
      <Ribbon className="ribbon--hero" />
      <div className="project-empty__copy">
        <h1>
          {failed ? '项目没有载入' : '选择项目目录'}
          <span>{failed ? '原文件未被改写' : '发现任务来源'}</span>
        </h1>
        <p>
          {failed
            ? '桌面端未能读取项目目录。检查路径和访问权限后重试；原文件没有被改写。'
            : '递归查找 TODO.md、TODO 和 TODO.txt，保留每项任务的来源。没有任务文件也能打开；普通清单只读预览，确认后才接管。'}
        </p>
        <div className="project-empty__actions">
          <button className="solid-action" type="button" onClick={onChooseProject}>
            <Icon name="folder" /> {failed ? '选择其他项目' : '选择项目'}
          </button>
          {failed && <button className="line-action" type="button" onClick={onRetry}>重新载入</button>}
        </div>
        <code>Project Root → Task Source → Project Task</code>
      </div>
    </section>
  );
}
