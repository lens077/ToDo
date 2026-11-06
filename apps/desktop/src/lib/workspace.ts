//! Workspace registry client (`docs/workspace-knowledge-phase0-audit.md`).
//!
//! Mirrors `lib/reading.ts`: typed wrappers over Tauri commands with an
//! in-memory mock for the browser build. The registry is the source of truth
//! for project identity — the mock keeps the same "relink keeps the ID"
//! contract so E2E exercises the real rules.

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export interface WorkspaceProject {
  id: string;
  root_path: string;
  name: string;
  path_status: 'reachable' | 'missing';
  created_at: number;
  updated_at: number;
  last_opened_at: number | null;
  workspace_ids: string[];
}

export interface WorkspaceSummary {
  id: string;
  name: string;
  description: string;
  created_at: number;
  updated_at: number;
  projects: WorkspaceProject[];
}

export interface AddedProject {
  project: WorkspaceProject;
  created: boolean;
}

export interface ProjectProfile {
  description: string;
  project_type: string;
  lifecycle_status: string;
  tech_stack: string[];
  current_milestone: string;
  progress_summary: string;
  blockers: string;
  next_action: string;
  updated_at: number;
}

export interface ObservedCommand {
  name: string;
  source: string;
}

export interface ObservedEnvironment {
  package_manager: string | null;
  runtimes: string[];
  commands: ObservedCommand[];
  services: string[];
  ports: string[];
  env_var_names: string[];
  source_files: string[];
}

export interface EnvironmentProfile {
  declared_json: string;
  observed: ObservedEnvironment | null;
  scanned_at: number | null;
  confirmed_at: number | null;
  status: string;
}

export interface WorkspaceError extends Error {
  /** Command-level failure with a message; shown as-is by the view. */
  message: string;
}

function errorOf(message: string): WorkspaceError {
  return { name: 'WorkspaceError', message } as WorkspaceError;
}

// ---------------------------------------------------------------------------
// Mock state (browser build / E2E)

let mockSequence = 1;
let mockWorkspaces: WorkspaceSummary[] = [];
let mockOpenedProjects = 0;

function mockDirName(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function mockStatus(path: string): WorkspaceProject['path_status'] {
  return mockMissingPaths.has(path) ? 'missing' : 'reachable';
}

const mockMissingPaths = new Set<string>();

function mockFind(projectId: string): WorkspaceProject {
  for (const workspace of mockWorkspaces) {
    const found = workspace.projects.find((project) => project.id === projectId);
    if (found) return found;
  }
  throw errorOf(`注册项目不存在: ${projectId}`);
}

export function resetWorkspaceMock(): void {
  mockWorkspaces = [];
  mockMissingPaths.clear();
  mockOpenedProjects = 0;
  for (const key of Object.keys(mockProfiles)) delete mockProfiles[key];
  for (const key of Object.keys(mockEnvironments)) delete mockEnvironments[key];
}

/** Mock-only: simulates a directory disappearing (moved/deleted). */
export function mockMarkMissing(path: string): void {
  mockMissingPaths.add(path);
}

// ---------------------------------------------------------------------------

export const workspace = {
  /** Directory picker. The mock answers with a synthetic path. */
  async pickDirectory(title: string): Promise<string | null> {
    if (isTauriRuntime()) {
      const selected = await open({ directory: true, multiple: false, title });
      return typeof selected === 'string' ? selected : null;
    }
    return `/mock/project-${mockSequence++}`;
  },

  async list(): Promise<WorkspaceSummary[]> {
    if (isTauriRuntime()) return call<WorkspaceSummary[]>('workspace_list');
    return mockWorkspaces.map((item) => ({
      ...item,
      projects: item.projects.map((project) => ({
        ...project,
        path_status: mockStatus(project.root_path),
      })),
    }));
  },

  async create(name: string, description = ''): Promise<WorkspaceSummary> {
    if (isTauriRuntime()) return call<WorkspaceSummary>('workspace_create', { name, description });
    if (!name.trim()) throw errorOf('工作空间名称不能为空');
    const now = Date.now();
    const item: WorkspaceSummary = {
      id: `wsmock${mockSequence++}`,
      name: name.trim(),
      description,
      created_at: now,
      updated_at: now,
      projects: [],
    };
    mockWorkspaces.push(item);
    return item;
  },

  async rename(id: string, name: string): Promise<WorkspaceSummary> {
    if (isTauriRuntime()) return call<WorkspaceSummary>('workspace_rename', { id, name });
    const item = mockWorkspaces.find((candidate) => candidate.id === id);
    if (!item) throw errorOf(`工作空间不存在: ${id}`);
    item.name = name.trim();
    item.updated_at = Date.now();
    return item;
  },

  async remove(id: string): Promise<void> {
    if (isTauriRuntime()) return call<void>('workspace_delete', { id });
    const before = mockWorkspaces.length;
    mockWorkspaces = mockWorkspaces.filter((candidate) => candidate.id !== id);
    if (mockWorkspaces.length === before) throw errorOf(`工作空间不存在: ${id}`);
  },

  async addProject(workspaceId: string, path: string): Promise<AddedProject> {
    if (isTauriRuntime()) return call<AddedProject>('workspace_add_project', { workspaceId, path });
    const item = mockWorkspaces.find((candidate) => candidate.id === workspaceId);
    if (!item) throw errorOf(`工作空间不存在: ${workspaceId}`);
    mockMissingPaths.delete(path);
    const existing = item.projects.find((project) => project.root_path === path);
    if (existing) return { project: existing, created: false };
    const now = Date.now();
    const project: WorkspaceProject = {
      id: `pmock${mockSequence++}`,
      root_path: path,
      name: mockDirName(path),
      path_status: 'reachable',
      created_at: now,
      updated_at: now,
      last_opened_at: null,
      workspace_ids: [workspaceId],
    };
    item.projects.push(project);
    return { project, created: true };
  },

  async removeProject(workspaceId: string, projectId: string): Promise<void> {
    if (isTauriRuntime()) return call<void>('workspace_remove_project', { workspaceId, projectId });
    const item = mockWorkspaces.find((candidate) => candidate.id === workspaceId);
    if (!item) throw errorOf(`工作空间不存在: ${workspaceId}`);
    const before = item.projects.length;
    item.projects = item.projects.filter((project) => project.id !== projectId);
    if (item.projects.length === before) throw errorOf(`注册项目不存在: ${projectId}`);
  },

  /** Re-links a project to a new directory. Identity (ID) is preserved. */
  async relinkProject(projectId: string, path: string): Promise<WorkspaceProject> {
    if (isTauriRuntime()) return call<WorkspaceProject>('workspace_relink_project', { projectId, path });
    for (const item of mockWorkspaces) {
      if (item.projects.some((project) => project.root_path === path && project.id !== projectId)) {
        throw errorOf(`路径已被其他项目注册: ${path}`);
      }
    }
    const project = mockFind(projectId);
    mockMissingPaths.delete(path);
    project.root_path = path;
    project.name = mockDirName(path);
    project.updated_at = Date.now();
    return project;
  },

  async deleteProject(projectId: string): Promise<void> {
    if (isTauriRuntime()) return call<void>('workspace_delete_project', { projectId });
    mockFind(projectId);
    for (const item of mockWorkspaces) {
      item.projects = item.projects.filter((project) => project.id !== projectId);
    }
  },

  /**
   * Mock-side record of opening a registered project. In Tauri the open is
   * `desktop.openRegisteredProject` (it must swap the live snapshot), the
   * browser build just records the intent so E2E can assert on it.
   */
  async openProject(projectId: string): Promise<void> {
    if (isTauriRuntime()) return;
    const project = mockFind(projectId);
    if (mockStatus(project.root_path) !== 'reachable') {
      throw errorOf(`项目目录不可达：${project.root_path}。请重新关联目录。`);
    }
    project.last_opened_at = Date.now();
    mockOpenedProjects += 1;
  },

  /** Mock-only probe for E2E assertions. */
  mockOpenedProjectCount(): number {
    return mockOpenedProjects;
  },

  // ── Project profile & environment ────────────────────────────────────────

  async getProfile(projectId: string): Promise<ProjectProfile> {
    if (isTauriRuntime()) return call<ProjectProfile>('project_profile_get', { projectId });
    return { ...emptyProfile(), ...mockProfiles[projectId] };
  },

  async setProfile(projectId: string, profile: ProjectProfile): Promise<ProjectProfile> {
    if (isTauriRuntime()) return call<ProjectProfile>('project_profile_set', { projectId, profile });
    mockFind(projectId);
    const saved = { ...profile, updated_at: Date.now() };
    mockProfiles[projectId] = saved;
    return saved;
  },

  async getEnvironment(projectId: string): Promise<EnvironmentProfile> {
    if (isTauriRuntime()) return call<EnvironmentProfile>('environment_get', { projectId });
    mockFind(projectId);
    return mockEnvironments[projectId] ?? { declared_json: '{}', observed: null, scanned_at: null, confirmed_at: null, status: 'unscanned' };
  },

  /**
   * Scans the registered root. Declared values are structurally separate, so
   * this can never overwrite a user declaration (mirrors the Rust gate).
   */
  async scanEnvironment(projectId: string): Promise<EnvironmentProfile> {
    if (isTauriRuntime()) return call<EnvironmentProfile>('environment_scan', { projectId });
    mockFind(projectId);
    const previous = mockEnvironments[projectId];
    const next: EnvironmentProfile = {
      declared_json: previous?.declared_json ?? '{}',
      observed: {
        package_manager: 'pnpm',
        runtimes: ['node', 'typescript'],
        commands: [
          { name: 'dev', source: 'package.json' },
          { name: 'test', source: 'package.json' },
          { name: 'check', source: 'Makefile' },
        ],
        services: ['postgres'],
        ports: ['3080:3080'],
        env_var_names: ['DATABASE_URL'],
        source_files: ['package.json', 'Makefile', '.env.example'],
      },
      scanned_at: Date.now(),
      confirmed_at: previous?.confirmed_at ?? null,
      status: 'scanned',
    };
    mockEnvironments[projectId] = next;
    return next;
  },

  async setDeclaredEnvironment(projectId: string, declaredJson: string): Promise<EnvironmentProfile> {
    if (isTauriRuntime()) return call<EnvironmentProfile>('environment_set_declared', { projectId, declaredJson });
    mockFind(projectId);
    let parsed: unknown;
    try {
      parsed = JSON.parse(declaredJson);
    } catch {
      throw errorOf('声明环境必须是合法 JSON');
    }
    const previous = mockEnvironments[projectId];
    const next: EnvironmentProfile = {
      declared_json: JSON.stringify(parsed),
      observed: previous?.observed ?? null,
      scanned_at: previous?.scanned_at ?? null,
      confirmed_at: Date.now(),
      status: 'confirmed',
    };
    mockEnvironments[projectId] = next;
    return next;
  },
};

function emptyProfile(): ProjectProfile {
  return {
    description: '',
    project_type: '',
    lifecycle_status: '',
    tech_stack: [],
    current_milestone: '',
    progress_summary: '',
    blockers: '',
    next_action: '',
    updated_at: 0,
  };
}

const mockProfiles: Record<string, ProjectProfile> = {};
const mockEnvironments: Record<string, EnvironmentProfile> = {};
