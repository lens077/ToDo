import * as React from 'react';
import type { LoadPhase } from '../hooks/useDesktop';
import { Icon } from './Icon';
import { Ribbon } from './Ribbon';

interface EmptyStateProps {
  phase: Extract<LoadPhase, 'loading' | 'no-project' | 'load-error'>;
  onChooseProject: () => void;
  onRetry: () => void;
  /** Onboarding-spec §3.1: explicit TODO.md creation; undefined disables it. */
  onCreateTodoMd?: () => void;
}

export function EmptyState({ phase, onChooseProject, onRetry, onCreateTodoMd }: EmptyStateProps) {
  const [createOpen, setCreateOpen] = React.useState(false);
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
        {!failed && (
          <div className="project-empty__actions">
            <button
              className="line-action"
              type="button"
              aria-expanded={createOpen}
              onClick={() => setCreateOpen((open) => !open)}
            >
              还没有 TODO.md？
            </button>
          </div>
        )}
        {createOpen && !failed && (
          <div className="project-empty__guide" role="note">
            <p>
              在已有仓库里，推荐先跑 <code>todo scan</code>：它会找出仓库里早已写下、却从未被跟踪的
              TODO 注释与 checkbox，且完全不写盘；或用 <code>todo init</code> 创建一份标准清单。
            </p>
            <p>
              也可以由桌面端创建：下一步会打开目录选择框，<strong>将在所选目录写入一份空的 TODO.md</strong>
              （列结构 Todo / Doing / Done）。这是显式动作，桌面端不会自行创建任何文件。
            </p>
            {onCreateTodoMd && (
              <button className="line-action" type="button" onClick={onCreateTodoMd}>
                选择目录并创建 TODO.md
              </button>
            )}
          </div>
        )}
        <code>Project Root → Task Source → Project Task</code>
      </div>
    </section>
  );
}
