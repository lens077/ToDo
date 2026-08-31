import type { LoadPhase } from '../hooks/useDesktop';
import { Icon } from './Icon';

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
      <div className="project-empty__diagram" aria-hidden="true">
        <span className="project-empty__paper"><i /><i /><i /></span>
        <span className="project-empty__connector" />
        <span className="project-empty__folder"><Icon name={failed ? 'alert' : 'folder'} /></span>
      </div>
      <p className="eyebrow">{failed ? 'PROJECT LOAD INTERRUPTED' : 'LOCAL FIRST / NO PROJECT'}</p>
      <h1>{failed ? '项目没有载入' : '选择一份真实的 TODO.md'}</h1>
      <p>
        {failed
          ? '文件存在，但桌面端没能构建 Snapshot。原文件没有被改写，可以修复后重试。'
          : '选择包含 TODO.md 的仓库。桌面端不会自动接管，也不会把界面缓存当作事实源。'}
      </p>
      <div className="project-empty__actions">
        <button className="solid-action" type="button" onClick={onChooseProject}>
          <Icon name="folder" /> {failed ? '选择其他项目' : '选择项目'}
        </button>
        {failed && <button className="line-action" type="button" onClick={onRetry}>重新载入</button>}
      </div>
      <code>Project Task → TODO.md → Git history</code>
    </section>
  );
}
