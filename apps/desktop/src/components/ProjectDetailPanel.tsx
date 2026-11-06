/**
 * 项目档案: profile fields (user-confirmed) plus the Declared/Observed
 * environment split. The scan button never rewrites the declared JSON — that
 * boundary lives in the store and the mock mirrors it, so the E2E can assert
 * "scan keeps declarations" against the same rule the Rust side enforces.
 */
import * as React from 'react';
import {
  knowledge,
  LOGICAL_TYPES,
  LOGICAL_TYPE_LABELS,
  type KnowledgeDocument,
  type KnowledgeDocumentContent,
  type LogicalType,
} from '../lib/knowledge';
import {
  workspace,
  type EnvironmentProfile,
  type ProjectProfile,
} from '../lib/workspace';
import { session, type SessionResume, type SessionSummary } from '../lib/session';
import { Icon } from './Icon';

const LIFECYCLE_OPTIONS = ['active', 'paused', 'maintenance', 'completed', 'archived'];
const PROJECT_TYPES = ['', 'web', 'desktop', 'backend', 'library', 'infra', 'game', 'other'];

interface ProjectDetailPanelProps {
  projectId: string;
  projectName: string;
}

export function ProjectDetailPanel({ projectId, projectName }: ProjectDetailPanelProps) {
  const [profile, setProfile] = React.useState<ProjectProfile | null>(null);
  const [environment, setEnvironment] = React.useState<EnvironmentProfile | null>(null);
  const [status, setStatus] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [declaredDraft, setDeclaredDraft] = React.useState('');
  const [documents, setDocuments] = React.useState<KnowledgeDocument[] | null>(null);
  const [openDocument, setOpenDocument] = React.useState<KnowledgeDocumentContent | null>(null);
  const [sessions, setSessions] = React.useState<SessionSummary[] | null>(null);
  const [sessionResume, setSessionResume] = React.useState<SessionResume | null>(null);
  const [sessionGoal, setSessionGoal] = React.useState('');
  const [finishDraft, setFinishDraft] = React.useState({
    completed: '',
    decisions: '',
    validation_command: '',
    validation_result: '',
    validation_evidence: '',
    unresolved: '',
    next_actions: '',
    task_ids: '',
    context_for_next: '',
  });

  React.useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [nextProfile, nextEnvironment] = await Promise.all([
          workspace.getProfile(projectId),
          workspace.getEnvironment(projectId),
        ]);
        if (cancelled) return;
        setProfile(nextProfile);
        setEnvironment(nextEnvironment);
        setDeclaredDraft(nextEnvironment.declared_json);
        const [sessionList, resumed] = await Promise.all([
          session.list(projectId),
          session.resume(projectId),
        ]);
        if (cancelled) return;
        setSessions(sessionList);
        setSessionResume(resumed);
      } catch (cause) {
        if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const saveProfile = React.useCallback(
    async (patch: Partial<ProjectProfile>) => {
      if (!profile) return;
      setBusy('profile');
      try {
        const saved = await workspace.setProfile(projectId, { ...profile, ...patch });
        setProfile(saved);
        setStatus('档案已保存');
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setBusy(null);
      }
    },
    [profile, projectId],
  );

  const scan = React.useCallback(async () => {
    setBusy('scan');
    try {
      const next = await workspace.scanEnvironment(projectId);
      setEnvironment(next);
      setDeclaredDraft(next.declared_json);
      setStatus('扫描完成（已保留声明环境）');
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [projectId]);

  const saveDeclared = React.useCallback(async () => {
    setBusy('declared');
    try {
      const next = await workspace.setDeclaredEnvironment(projectId, declaredDraft);
      setEnvironment(next);
      setStatus('声明环境已确认');
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [declaredDraft, projectId]);

  const scanDocuments = React.useCallback(async () => {
    setBusy('knowledge');
    try {
      const report = await knowledge.scan(projectId);
      setDocuments(report.documents);
      setOpenDocument(null);
      setStatus(`文档扫描完成：${report.scanned} 个文件，建议 ${report.indexed} 项分类`);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [projectId]);

  const openDoc = React.useCallback(async (documentId: string) => {
    setBusy(`doc:${documentId}`);
    try {
      setOpenDocument(await knowledge.read(documentId));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, []);

  const confirmType = React.useCallback(async (documentId: string, logicalType: LogicalType) => {
    setBusy(`confirm:${documentId}`);
    try {
      const confirmed = await knowledge.confirm(documentId, logicalType);
      setDocuments((current) => current?.map((doc) => (doc.id === confirmed.id ? confirmed : doc)) ?? current);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, []);

  const refreshSessions = React.useCallback(async () => {
    const [sessionList, resumed] = await Promise.all([session.list(projectId), session.resume(projectId)]);
    setSessions(sessionList);
    setSessionResume(resumed);
  }, [projectId]);

  const beginSession = React.useCallback(async () => {
    setBusy('session:begin');
    try {
      await session.begin(projectId, sessionGoal);
      setSessionGoal('');
      await refreshSessions();
      setStatus('会话已开始');
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [projectId, refreshSessions, sessionGoal]);

  const lines = (value: string) => value.split('\n').map((line) => line.trim()).filter(Boolean);

  const finishSession = React.useCallback(async () => {
    setBusy('session:finish');
    try {
      await session.finish(projectId, {
        completed: lines(finishDraft.completed),
        decisions: lines(finishDraft.decisions),
        validation: finishDraft.validation_command.trim()
          ? [{
              command: finishDraft.validation_command,
              result: finishDraft.validation_result,
              evidence: finishDraft.validation_evidence,
            }]
          : [],
        unresolved: lines(finishDraft.unresolved),
        next_actions: lines(finishDraft.next_actions),
        task_ids: finishDraft.task_ids.split(/[,\s，、]+/).map((id) => id.trim()).filter(Boolean),
        context_for_next: finishDraft.context_for_next,
      });
      await refreshSessions();
      setStatus('会话总结已写入 .virgo/sessions/');
      setFinishDraft({
        completed: '', decisions: '', validation_command: '', validation_result: '',
        validation_evidence: '', unresolved: '', next_actions: '', task_ids: '', context_for_next: '',
      });
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [finishDraft, projectId, refreshSessions]);

  const interruptSession = React.useCallback(async () => {
    setBusy('session:interrupt');
    try {
      await session.interrupt(projectId);
      await refreshSessions();
      setStatus('会话已标记为中断');
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [projectId, refreshSessions]);

  if (!profile || !environment) {
    return <p className="ai-loading" role="status">{error ?? '正在载入项目档案…'}</p>;
  }

  const field = (label: string, key: keyof ProjectProfile, placeholder: string) => (
    <label className="ws-field">
      <span>{label}</span>
      <input
        value={String(profile[key] ?? '')}
        placeholder={placeholder}
        onChange={(event) => setProfile({ ...profile, [key]: event.target.value })}
        onBlur={() => {
          if (profile[key] !== undefined) void saveProfile({ [key]: profile[key] });
        }}
      />
    </label>
  );

  return (
    <div className="ws-detail" data-testid="project-detail">
      <div className="ws-detail__section">
        <h4>项目档案</h4>
        {field('项目目标', 'description', '这个项目要解决什么问题？')}
        {field('当前里程碑', 'current_milestone', '例如：v1.2 桌面端发布')}
        {field('下一步', 'next_action', '明确的下一步动作')}
        {field('阻塞项', 'blockers', '当前被什么阻塞？')}
        <div className="ws-field-row">
          <label className="ws-field">
            <span>项目类型</span>
            <select
              value={profile.project_type}
              onChange={(event) => void saveProfile({ project_type: event.target.value })}
            >
              {PROJECT_TYPES.map((type) => (
                <option key={type} value={type}>{type || '未设置'}</option>
              ))}
            </select>
          </label>
          <label className="ws-field">
            <span>生命周期</span>
            <select
              value={profile.lifecycle_status || 'active'}
              onChange={(event) => void saveProfile({ lifecycle_status: event.target.value })}
            >
              {LIFECYCLE_OPTIONS.map((option) => (
                <option key={option} value={option}>{option}</option>
              ))}
            </select>
          </label>
        </div>
        {profile.tech_stack.length > 0 && (
          <p className="ws-project__meta">技术栈：{profile.tech_stack.join(' · ')}</p>
        )}
      </div>

      <div className="ws-detail__section">
        <h4>环境</h4>
        <div className="ws-env__toolbar">
          <button type="button" className="ws-tool" disabled={busy !== null} onClick={() => void scan()}>
            <Icon name="refresh" /> 扫描环境
          </button>
          <span className="ws-project__meta">
            {environment.status === 'unscanned' && '尚未扫描'}
            {environment.status !== 'unscanned' && environment.scanned_at && (
              <>扫描时间 {new Date(environment.scanned_at).toLocaleString()}</>
            )}
          </span>
        </div>
        <div className="ws-env__split">
          <div>
            <h5>声明（用户确认）</h5>
            <textarea
              className="ws-env__declared"
              value={declaredDraft}
              rows={4}
              spellCheck={false}
              onChange={(event) => setDeclaredDraft(event.target.value)}
            />
            <button type="button" className="button-primary" disabled={busy !== null} onClick={() => void saveDeclared()}>
              确认声明
            </button>
          </div>
          <div>
            <h5>观测（只读扫描）</h5>
            {environment.observed ? (
              <ul className="ws-env__observed">
                {environment.observed.package_manager && <li>包管理器：{environment.observed.package_manager}</li>}
                {environment.observed.runtimes.length > 0 && <li>运行时：{environment.observed.runtimes.join(' · ')}</li>}
                {environment.observed.commands.length > 0 && (
                  <li>命令：{environment.observed.commands.map((command) => command.name).join(' · ')}</li>
                )}
                {environment.observed.services.length > 0 && <li>服务：{environment.observed.services.join(' · ')}</li>}
                {environment.observed.ports.length > 0 && <li>端口：{environment.observed.ports.join(' · ')}</li>}
                {environment.observed.env_var_names.length > 0 && (
                  <li>环境变量（仅名称）：{environment.observed.env_var_names.join(' · ')}</li>
                )}
                <li className="ws-project__meta">来源：{environment.observed.source_files.join(' · ')}</li>
              </ul>
            ) : (
              <p className="ai-empty">还没有扫描结果。</p>
            )}
          </div>
        </div>
      </div>

      <div className="ws-detail__section">
        <h4>知识文档</h4>
        <div className="ws-env__toolbar">
          <button type="button" className="ws-tool" disabled={busy !== null} onClick={() => void scanDocuments()}>
            <Icon name="folder" /> 扫描文档
          </button>
          <span className="ws-project__meta">Markdown 原文件始终保留，索引只读。</span>
        </div>
        {documents && documents.length > 0 && (
          <ul className="ws-docs" data-testid="knowledge-docs">
            {documents.map((doc) => (
              <li key={doc.id} className={doc.tombstone ? 'ws-doc ws-doc--tombstone' : 'ws-doc'}>
                <button type="button" className="ws-doc__open" disabled={doc.tombstone || busy !== null} onClick={() => void openDoc(doc.id)}>
                  <strong>{doc.title}</strong>
                  <span className="ws-project__meta">{doc.relative_path}</span>
                </button>
                <select
                  className="ws-doc__type"
                  value={doc.logical_type}
                  aria-label={`文档类型 ${doc.title}`}
                  onChange={(event) => void confirmType(doc.id, event.target.value as LogicalType)}
                >
                  {LOGICAL_TYPES.map((type) => (
                    <option key={type} value={type}>{LOGICAL_TYPE_LABELS[type]}</option>
                  ))}
                </select>
                <span className={`ws-badge ${doc.classification_source === 'confirmed' ? 'ws-badge--active' : 'ws-badge--reachable'}`}>
                  {doc.classification_source === 'confirmed' ? '已确认' : '建议'}
                </span>
                {doc.tombstone && <span className="ws-badge ws-badge--missing">文件已删除</span>}
              </li>
            ))}
          </ul>
        )}
        {documents && documents.length === 0 && <p className="ai-empty">没有找到 Markdown 文档。</p>}
        {openDocument && (
          <article className="ws-reader" data-testid="knowledge-reader">
            <h3>{openDocument.title}</h3>
            <p className="ws-project__meta">
              {openDocument.relative_path} · {LOGICAL_TYPE_LABELS[openDocument.logical_type]} · HTML 由 Markdown 重建，不落盘
            </p>
            <div className="ws-reader__content" dangerouslySetInnerHTML={{ __html: openDocument.content_html }} />
          </article>
        )}
      </div>

      <div className="ws-detail__section">
        <h4>开发会话</h4>
        {sessionResume?.latest && (
          <div className="ws-session__resume" data-testid="session-resume">
            <span className={`ws-badge ${sessionResume.latest.status === 'active' ? 'ws-badge--active' : sessionResume.latest.status === 'completed' ? 'ws-badge--reachable' : 'ws-badge--missing'}`}>
              {sessionResume.latest.status === 'active' ? '进行中' : sessionResume.latest.status === 'completed' ? '已完成' : '已中断'}
            </span>
            <span className="ws-session__goal">{sessionResume.latest.goal}</span>
            {sessionResume.outstanding.length > 0 && (
              <span className="ws-project__meta">待续：{sessionResume.outstanding.join('；')}</span>
            )}
            {sessionResume.linked_tasks.length > 0 && (
              <ul className="ws-session__links">
                {sessionResume.linked_tasks.map((task) => (
                  <li key={task.id}>
                    <code>{task.id}</code>
                    {task.found ? (
                      <span>
                        {task.completed ? '✓ 已完成' : '○ 未完成'}
                        {task.column ? ` · ${task.column}` : ''} · {task.body}
                      </span>
                    ) : (
                      <span className="ws-project__meta">未在根 TODO.md 找到</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        {sessions && sessions.length > 0 && (
          <ul className="ws-docs" data-testid="session-list">
            {sessions.map((entry) => (
              <li key={entry.id} className="ws-doc">
                <span className="ws-doc__open">
                  <strong>{new Date(entry.started_at).toLocaleString()}</strong>
                  <span className="ws-project__meta">{entry.goal}</span>
                </span>
                <span className={`ws-badge ${entry.status === 'active' ? 'ws-badge--active' : entry.status === 'completed' ? 'ws-badge--reachable' : 'ws-badge--missing'}`}>
                  {entry.status === 'active' ? '进行中' : entry.status === 'completed' ? '已完成' : '已中断'}
                </span>
              </li>
            ))}
          </ul>
        )}
        {(!sessions || sessions.length === 0) && <p className="ai-empty">还没有会话记录。开始一个会话，跨天恢复上下文。</p>}

        {sessions?.some((entry) => entry.status === 'active') ? (
          <div className="ws-session__form">
            <div className="ws-field-row">
              <label className="ws-field">
                <span>已完成（每行一项）</span>
                <textarea rows={2} value={finishDraft.completed} onChange={(event) => setFinishDraft({ ...finishDraft, completed: event.target.value })} />
              </label>
              <label className="ws-field">
                <span>决策（每行一项）</span>
                <textarea rows={2} value={finishDraft.decisions} onChange={(event) => setFinishDraft({ ...finishDraft, decisions: event.target.value })} />
              </label>
            </div>
            <div className="ws-field-row ws-field-row--three">
              <label className="ws-field">
                <span>验证命令</span>
                <input value={finishDraft.validation_command} placeholder="make check-quick" onChange={(event) => setFinishDraft({ ...finishDraft, validation_command: event.target.value })} />
              </label>
              <label className="ws-field">
                <span>结果</span>
                <input value={finishDraft.validation_result} placeholder="绿" onChange={(event) => setFinishDraft({ ...finishDraft, validation_result: event.target.value })} />
              </label>
              <label className="ws-field">
                <span>证据</span>
                <input value={finishDraft.validation_evidence} placeholder="148s" onChange={(event) => setFinishDraft({ ...finishDraft, validation_evidence: event.target.value })} />
              </label>
            </div>
            <div className="ws-field-row">
              <label className="ws-field">
                <span>未解决（每行一项）</span>
                <textarea rows={2} value={finishDraft.unresolved} onChange={(event) => setFinishDraft({ ...finishDraft, unresolved: event.target.value })} />
              </label>
              <label className="ws-field">
                <span>下一步（每行一项）</span>
                <textarea rows={2} value={finishDraft.next_actions} onChange={(event) => setFinishDraft({ ...finishDraft, next_actions: event.target.value })} />
              </label>
            </div>
            <label className="ws-field">
              <span>给下次会话的上下文</span>
              <textarea rows={2} value={finishDraft.context_for_next} onChange={(event) => setFinishDraft({ ...finishDraft, context_for_next: event.target.value })} />
            </label>
            <label className="ws-field">
              <span>关联任务 ID（逗号分隔，resume 时解析当前状态）</span>
              <input
                value={finishDraft.task_ids}
                placeholder="如 1pevv2, h4m9zc"
                onChange={(event) => setFinishDraft({ ...finishDraft, task_ids: event.target.value })}
              />
            </label>
            <div className="ws-session__actions">
              <button type="button" className="button-primary" disabled={busy !== null} onClick={() => void finishSession()}>
                完成会话
              </button>
              <button type="button" className="ws-tool" disabled={busy !== null} onClick={() => void interruptSession()}>
                中断
              </button>
            </div>
          </div>
        ) : (
          <div className="ws-session__begin">
            <input
              value={sessionGoal}
              placeholder="本次会话目标，例如：接入 watcher 增量扫描"
              onChange={(event) => setSessionGoal(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && sessionGoal.trim()) void beginSession();
              }}
            />
            <button type="button" className="button-primary" disabled={busy !== null || !sessionGoal.trim()} onClick={() => void beginSession()}>
              <Icon name="plus" /> 开始会话
            </button>
          </div>
        )}
      </div>

      {status && <p className="ai-notice" role="status">{status}</p>}
      {error && <p className="ai-notice" role="alert">{error}</p>}
      <p className="ws-project__meta">项目 {projectName} · 档案更新于 {profile.updated_at ? new Date(profile.updated_at).toLocaleString() : '从未'}</p>
    </div>
  );
}
