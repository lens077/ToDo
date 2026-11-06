/**
 * Overview (Phase 7): the cross-project dashboard. One screen answering
 * "每个项目现在什么状态、下一步做什么" — totals row, filter chips, and one
 * card per registered project. Read-only aggregate; actions live in the
 * 工作空间 view.
 */
import * as React from 'react';
import { overview, type OverviewReport, type ProjectOverview } from '../lib/overview';
import { Icon } from './Icon';

type OverviewFilter = 'all' | 'active' | 'missing' | 'in-flight';

const FILTERS: Array<{ id: OverviewFilter; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'active', label: '活跃' },
  { id: 'in-flight', label: '有进行中会话' },
  { id: 'missing', label: '目录缺失' },
];

export function OverviewView() {
  const [report, setReport] = React.useState<OverviewReport | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [filter, setFilter] = React.useState<OverviewFilter>('all');

  const refresh = React.useCallback(async () => {
    try {
      setReport(await overview.summary());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  if (report === null) {
    return (
      <section className="reading-view">
        <p className="ai-loading" role="status">{error ?? '正在载入汇总面板…'}</p>
      </section>
    );
  }

  const filtered = report.projects.filter((project) => {
    if (filter === 'active') return project.path_status === 'reachable' && project.lifecycle_status === 'active';
    if (filter === 'missing') return project.path_status !== 'reachable';
    if (filter === 'in-flight') return project.session_status === 'active';
    return true;
  });

  return (
    <section className="reading-view ov-view" data-testid="overview-view">
      <div className="ov-toolbar">
        <h2 className="ws-title">总览</h2>
        <button type="button" className="ws-tool" onClick={() => void refresh()}>
          <Icon name="refresh" /> 刷新
        </button>
      </div>

      <div className="ov-totals" data-testid="overview-totals">
        <div className="ov-total"><strong>{report.total_projects}</strong><span>项目</span></div>
        <div className="ov-total ov-total--ok"><strong>{report.total_projects - report.missing_projects}</strong><span>路径正常</span></div>
        {report.missing_projects > 0 && (
          <div className="ov-total ov-total--warn"><strong>{report.missing_projects}</strong><span>目录缺失</span></div>
        )}
        <div className="ov-total ov-total--ok"><strong>{report.active_sessions}</strong><span>进行中会话</span></div>
        {report.unclassified_documents > 0 && (
          <div className="ov-total ov-total--warn"><strong>{report.unclassified_documents}</strong><span>未分类文档</span></div>
        )}
      </div>

      <div className="reading-filters" role="group" aria-label="按状态筛选">
        {FILTERS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={filter === item.id ? 'is-active' : undefined}
            onClick={() => setFilter(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      {filtered.length === 0 ? (
        <p className="ai-empty">
          {report.projects.length === 0
            ? '还没有注册项目。到「工作空间」添加一个真实目录开始。'
            : '这个筛选下没有项目。'}
        </p>
      ) : (
        <div className="ov-cards">
          {filtered.map((project) => (
            <OverviewCard project={project} key={project.id} />
          ))}
        </div>
      )}
      {error && <p className="ai-notice" role="alert">{error}</p>}
    </section>
  );
}

function OverviewCard({ project }: { project: ProjectOverview }) {
  const missing = project.path_status !== 'reachable';
  return (
    <article className={`ov-card${missing ? ' ov-card--missing' : ''}`} data-testid="overview-card">
      <div className="ws-project__title-row">
        <strong className="ws-project__name">{project.name}</strong>
        <span className={`ws-badge ws-badge--${project.path_status}`}>{missing ? '目录缺失' : '正常'}</span>
        <span className="ws-badge ws-badge--active">{project.lifecycle_status}</span>
        {project.session_status === 'active' && <span className="ws-badge ws-badge--active">会话进行中</span>}
      </div>
      <dl className="ov-card__fields">
        <div>
          <dt>里程碑</dt>
          <dd>{project.current_milestone || '—'}</dd>
        </div>
        <div>
          <dt>下一步</dt>
          <dd>{project.outstanding.length > 0 ? project.outstanding.join('；') : project.next_action || '—'}</dd>
        </div>
        {project.blockers && (
          <div>
            <dt>阻塞</dt>
            <dd className="ov-card__blockers">{project.blockers}</dd>
          </div>
        )}
        <div>
          <dt>文档</dt>
          <dd>
            {project.knowledge_documents}
            {project.unclassified_documents > 0 && (
              <span className="ov-card__hint"> · {project.unclassified_documents} 个待确认分类</span>
            )}
          </dd>
        </div>
      </dl>
    </article>
  );
}
