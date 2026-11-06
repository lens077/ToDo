//! Overview client (Phase 7). One aggregate call; the mock composes the
//! workspace/session/knowledge mocks so the E2E dashboard reflects the same
//! fixtures those specs create.

import { invoke } from '@tauri-apps/api/core';
import { isTauriRuntime } from './workspace';
import { session } from './session';
import { knowledge } from './knowledge';
import { workspace, type WorkspaceProject } from './workspace';

export interface ProjectOverview {
  id: string;
  name: string;
  root_path: string;
  path_status: 'reachable' | 'missing';
  lifecycle_status: string;
  current_milestone: string;
  next_action: string;
  blockers: string;
  session_status: 'active' | 'completed' | 'interrupted' | null;
  session_goal: string | null;
  outstanding: string[];
  knowledge_documents: number;
  unclassified_documents: number;
}

export interface OverviewReport {
  projects: ProjectOverview[];
  total_projects: number;
  missing_projects: number;
  active_sessions: number;
  unclassified_documents: number;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

/** Per-project aggregation the mock performs in place of the SQL/FS backend. */
async function mockProjectOverview(project: WorkspaceProject): Promise<ProjectOverview> {
  const [profile, resume, documents] = await Promise.all([
    workspace.getProfile(project.id),
    session.resume(project.id),
    knowledge.list(project.id),
  ]);
  return {
    id: project.id,
    name: project.name,
    root_path: project.root_path,
    path_status: project.path_status,
    lifecycle_status: profile.lifecycle_status || 'active',
    current_milestone: profile.current_milestone,
    next_action: profile.next_action,
    blockers: profile.blockers,
    session_status: resume.latest?.status ?? null,
    session_goal: resume.latest?.goal ?? null,
    outstanding: resume.outstanding,
    knowledge_documents: documents.filter((doc) => !doc.tombstone).length,
    unclassified_documents: documents.filter(
      (doc) => !doc.tombstone && doc.classification_source !== 'confirmed' && doc.logical_type === 'general',
    ).length,
  };
}

export const overview = {
  async summary(): Promise<OverviewReport> {
    if (isTauriRuntime()) return call<OverviewReport>('overview_summary');
    const workspaces = await workspace.list();
    const projects = workspaces.flatMap((item) => item.projects);
    // Dedup by stable ID: a project shared across workspaces is one card.
    const unique = new Map<string, WorkspaceProject>();
    for (const project of projects) unique.set(project.id, project);
    const overviews = await Promise.all([...unique.values()].map(mockProjectOverview));
    return {
      projects: overviews,
      total_projects: overviews.length,
      missing_projects: overviews.filter((item) => item.path_status !== 'reachable').length,
      active_sessions: overviews.filter((item) => item.session_status === 'active').length,
      unclassified_documents: overviews.reduce((sum, item) => sum + item.unclassified_documents, 0),
    };
  },
};
