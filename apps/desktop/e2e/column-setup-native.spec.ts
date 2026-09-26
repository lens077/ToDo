import { expect, test, type Page } from '@playwright/test';
import type { ColumnSetupPreview, DesktopSnapshot } from '../src/lib/desktop';

const root = '/fixtures/native-columns';
const source = 'docs/TODO.md';
const otherSource = 'notes/TODO.md';
const otherRoot = '/fixtures/other-project';
const taskName = 'Native columnless task';

function state(projectPath = root): DesktopSnapshot {
  const tasks = [source, otherSource].map((source_path, index) => ({
    source_path, id: 'native01', parent_id: null, body: index === 0 ? taskName : 'Native second task',
    completed: false, column: null, indent_level: 0, due: null, priority: 2 as const, tags: [],
    progress: null, failed_on: null, note: null, evidence: null,
    evidence_availability: { status: 'unavailable' as const, reason: 'No evidence in fixture' },
  }));
  return {
    project_path: projectPath, revision: 'aaaaaaaaaaaaaaaa', completion_policy: 'normal',
    sources: [source, otherSource].map((path) => ({ path, status: 'managed', writable: true, preview: '', revision: 'file-original', task_count: 1 })),
    selected_source: null, warnings: [], columns: [], tasks,
    today: { overdue: [], today: [], upcoming: [], next: tasks },
  };
}

interface NativeColumnsHarness {
  calls(): Array<{ command: string; args: Record<string, unknown> }>;
  hold(command: string): void;
  pending(command: string): boolean;
  release(command: string): void;
  switchProject(): void;
  notify(): void;
  setWritable(writable: boolean): void;
}
declare global { interface Window { __nativeColumns: NativeColumnsHarness } }

async function nativeFixture(page: Page) {
  await page.addInitScript(({ initial, other, source, otherSource }) => {
    for (const value of [initial, other]) localStorage.setItem(`todo.source-enrollment.v1:${value.project_path}`, JSON.stringify({ enrolled: [source, otherSource], seen: [source, otherSource] }));
    let disk = structuredClone(initial);
    let callbackSequence = 0;
    let revision = 0;
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const callbacks = new Map<number, (value: unknown) => void>();
    const listeners = new Map<string, Set<number>>();
    const holds = new Set<string>();
    const pending = new Map<string, () => void>();
    const answer = async (command: string, value: unknown) => {
      const captured = structuredClone(value);
      if (holds.delete(command)) await new Promise<void>((resolve) => pending.set(command, resolve));
      return captured;
    };
    const snapshot = (selected: unknown = null) => ({
      ...disk, selected_source: selected,
      tasks: disk.tasks.filter((task) => selected === null || task.source_path === selected),
      columns: disk.columns.filter((column) => selected === null || column.source_path === selected),
      today: { ...disk.today, next: disk.today.next.filter((task) => selected === null || task.source_path === selected) },
    });
    const notify = () => {
      const event = 'desktop://snapshot-changed';
      for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload: structuredClone(disk) });
    };
    const unregisterListener = (event: string, id: number) => { listeners.get(event)?.delete(id); callbacks.delete(id); };
    Object.assign(window, {
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
        transformCallback(callback: (value: unknown) => void, once = false) {
          const id = ++callbackSequence;
          callbacks.set(id, (value) => { if (once) callbacks.delete(id); callback(value); });
          return id;
        },
        unregisterCallback(id: number) { callbacks.delete(id); },
        runCallback(id: number, value: unknown) { callbacks.get(id)?.(value); },
        async invoke(command: string, args: Record<string, unknown> = {}) {
          calls.push({ command, args: structuredClone(args) });
          switch (command) {
            case 'desktop_status': return { startup_ms: 1, shortcut_status: { status: 'registered' } };
            case 'capture_list': return [];
            case 'ai_list': return { records: [], connected: false };
            case 'snapshot': return answer(command, snapshot());
            case 'project_snapshot': return answer(command, snapshot(args.sourcePath ?? null));
            case 'column_setup_options':
              if (args.expectedProjectPath !== disk.project_path) throw new Error('project context conflict');
              return answer(command, { source_path: args.sourcePath, base_hash: 'file-original', configured: false, headings: [{ name: '领域', line: 5, selectable: true, reason: null }] });
            case 'preview_columns': {
              if (args.expectedProjectPath !== disk.project_path) throw new Error('project context conflict');
              const preview: ColumnSetupPreview = {
                source_path: String(args.sourcePath), base_hash: 'file-original', selection: args.selection as ColumnSetupPreview['selection'],
                columns: [{ id: 'todo', name: '待办', done: false, created: true }, { id: 'doing', name: '进行中', done: false, created: true }, { id: 'done', name: '已完成', done: true, created: true }],
                proposed: 'only generated by backend', diff: { added: 1, removed: 0, hunks: [{ kind: 'added', left_line: null, right_line: 1, text: 'columns:' }] },
              };
              return answer(command, preview);
            }
            case 'apply_columns': {
              if (args.expectedProjectPath !== disk.project_path) throw new Error('project context conflict');
              disk = { ...disk, revision: `written-${++revision}`, columns: [...disk.columns, ...[
                { id: 'todo', name: '待办', done: false }, { id: 'doing', name: '进行中', done: false }, { id: 'done', name: '已完成', done: true },
              ].map((column) => ({ ...column, source_path: String(args.sourcePath) }))] };
              const selection = args.selection as ColumnSetupPreview['selection'];
              if (selection.mode === 'defaults_and_start') disk.tasks = disk.tasks.map((task) => task.source_path === args.sourcePath && task.id === selection.task_id ? { ...task, column: 'doing' } : task);
              return answer(command, snapshot());
            }
            case 'project_mutate': {
              const intent = args.intent as { type: string; id: string; priority: 1 | 2 | 3 | 4 | null };
              if (intent.type !== 'set_priority') throw new Error('Unexpected task mutation');
              disk = { ...disk, revision: `mutated-${++revision}`, tasks: disk.tasks.map((task) => task.source_path === args.sourcePath && task.id === intent.id ? { ...task, priority: intent.priority } : task) };
              return answer(command, snapshot());
            }
            case 'plugin:event|listen': {
              const event = String(args.event), id = Number(args.handler);
              const handlers = listeners.get(event) ?? new Set<number>();
              handlers.add(id); listeners.set(event, handlers); return id;
            }
            case 'plugin:event|unlisten': unregisterListener(String(args.event), Number(args.eventId ?? args.id)); return null;
            default: throw new Error(`Unexpected native fixture command: ${command}`);
          }
        },
      },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener },
    });
    window.__nativeColumns = {
      calls: () => structuredClone(calls),
      hold: (command) => { holds.add(command); },
      pending: (command) => pending.has(command),
      release(command) {
        const release = pending.get(command);
        if (!release) throw new Error(`No held ${command}`);
        pending.delete(command); release();
      },
      switchProject() { disk = structuredClone(other); notify(); },
      notify,
      setWritable(writable) {
        disk = { ...disk, revision: `permission-${++revision}`, sources: disk.sources!.map((entry) => entry.path === source ? { ...entry, writable } : entry) };
        notify();
      },
    };
  }, { initial: state(), other: state(otherRoot), source, otherSource });
  await page.goto('/');
  await expect(page.getByRole('article', { name: `未完成 Project Task：${taskName}`, exact: true })).toBeVisible();
}

const card = (page: Page, name = taskName) => page.getByRole('article', { name: `未完成 Project Task：${name}`, exact: true });
const dialog = (page: Page) => page.getByRole('dialog', { name: '配置任务列', exact: true });
async function open(page: Page) {
  await card(page).click({ button: 'right' });
  await page.getByRole('menuitem', { name: '配置任务列…', exact: true }).click();
  await expect(dialog(page).getByRole('button', { name: '预览差异', exact: true })).toBeEnabled();
}
async function hold(page: Page, command: string) { await page.evaluate((value) => window.__nativeColumns.hold(value), command); }
async function pending(page: Page, command: string) { await expect.poll(() => page.evaluate((value) => window.__nativeColumns.pending(value), command)).toBe(true); }
async function release(page: Page, command: string) {
  await page.evaluate((value) => window.__nativeColumns.release(value), command);
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
}
async function calls(page: Page, command: string) { return page.evaluate((value) => window.__nativeColumns.calls().filter((call) => call.command === value), command); }

test('native direct-start drop sends only source-local task ID and one atomic selection', async ({ page }) => {
  await nativeFixture(page);
  await card(page).dragTo(page.locator('.goal-region--doing'));
  const start = page.getByRole('dialog', { name: '创建列并推进任务', exact: true });
  await expect(start.getByRole('table', { name: '左右对比' })).toBeVisible();
  expect((await calls(page, 'preview_columns'))[0].args).toEqual({ sourcePath: source, selection: { mode: 'defaults_and_start', task_id: 'native01' }, expectedProjectPath: root });
  await start.getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(start).toHaveCount(0);
  await expect(page.locator('.goal-region--doing').getByRole('article', { name: `未完成 Project Task：${taskName}`, exact: true })).toBeVisible();
  expect((await calls(page, 'apply_columns'))[0].args).toEqual({ sourcePath: source, baseHash: 'file-original', selection: { mode: 'defaults_and_start', task_id: 'native01' }, expectedProjectPath: root });
  expect(await calls(page, 'project_mutate')).toHaveLength(0);
});

test('direct-start pending preview is discarded when its source changes', async ({ page }) => {
  await nativeFixture(page);
  await hold(page, 'preview_columns');
  await card(page).dragTo(page.locator('.goal-region--doing'));
  const start = page.getByRole('dialog', { name: '创建列并推进任务', exact: true });
  await pending(page, 'preview_columns');
  await page.getByRole('combobox', { name: '任务来源', exact: true, includeHidden: true }).selectOption(otherSource, { force: true });
  await expect(start).toHaveCount(0);
  await release(page, 'preview_columns');
  expect(await calls(page, 'apply_columns')).toHaveLength(0);
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toHaveValue(otherSource);
});

test('native bridge invokes exact commands and pins project on options, preview and apply', async ({ page }) => {
  await nativeFixture(page);
  await open(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await calls(page, 'column_setup_options'))[0].args).toEqual({ sourcePath: source, expectedProjectPath: root });
  expect((await calls(page, 'preview_columns'))[0].args).toEqual({ sourcePath: source, selection: { mode: 'defaults' }, expectedProjectPath: root });
  expect((await calls(page, 'apply_columns'))[0].args).toEqual({ sourcePath: source, baseHash: 'file-original', selection: { mode: 'defaults' }, expectedProjectPath: root });
  await card(page).click({ button: 'right' });
  await page.getByRole('menuitem', { name: '移动到…', exact: true }).click();
  await expect(page.getByRole('menu', { name: '移动到列' }).getByRole('menuitem')).toHaveCount(3);
});

for (const stage of ['preview_columns', 'apply_columns']) {
  test(`native project switch closes old dialog and ignores late ${stage}`, async ({ page }) => {
    await nativeFixture(page);
    await open(page);
    await hold(page, stage);
    await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
    if (stage === 'apply_columns') await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
    await pending(page, stage);
    await page.evaluate(() => window.__nativeColumns.switchProject());
    await expect(dialog(page)).toHaveCount(0);
    await expect(page.getByRole('button', { name: '当前项目 other-project', exact: true })).toBeVisible();
    await release(page, stage);
    await expect(page.getByRole('button', { name: '当前项目 other-project', exact: true })).toBeVisible();
    await open(page);
    await expect(dialog(page)).toContainText(otherRoot);
    await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
    expect(await calls(page, 'apply_columns')).toHaveLength(stage === 'apply_columns' ? 1 : 0);
  });
}

test('source switch invalidates a preview in flight and ignores its late response', async ({ page }) => {
  await nativeFixture(page);
  await open(page);
  await hold(page, 'preview_columns');
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await pending(page, 'preview_columns');
  // A native menu/shortcut may change source while a webview modal is open.
  // Use the actual select change handler, not React/module state injection.
  await page.getByRole('combobox', { name: '任务来源', exact: true, includeHidden: true }).selectOption(otherSource, { force: true });
  await expect(dialog(page)).toHaveCount(0);
  await release(page, 'preview_columns');
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toHaveValue(otherSource);
  await expect(card(page, 'Native second task')).toBeVisible();
  expect(await calls(page, 'apply_columns')).toHaveLength(0);
});

test('queued configuration is abandoned on source switch before the write starts', async ({ page }) => {
  await nativeFixture(page);
  await hold(page, 'project_mutate');
  await card(page, 'Native second task').press('1');
  await pending(page, 'project_mutate');
  await open(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page).getByRole('button', { name: '写入中…', exact: true })).toBeDisabled();
  expect(await calls(page, 'apply_columns')).toHaveLength(0);
  await page.getByRole('combobox', { name: '任务来源', exact: true, includeHidden: true }).selectOption(otherSource, { force: true });
  await expect(dialog(page)).toHaveCount(0);
  await release(page, 'project_mutate');
  expect(await calls(page, 'apply_columns')).toHaveLength(0);
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toHaveValue(otherSource);
});

test('queued configuration rechecks source permissions before dispatching its write', async ({ page }) => {
  await nativeFixture(page);
  await hold(page, 'project_mutate');
  await card(page, 'Native second task').press('1');
  await pending(page, 'project_mutate');
  await open(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await page.evaluate(() => window.__nativeColumns.setWritable(false));
  await expect(dialog(page)).toContainText('只读');
  await release(page, 'project_mutate');
  await expect(dialog(page).getByRole('alert')).toContainText('重新读取');
  expect(await calls(page, 'apply_columns')).toHaveLength(0);
});

test('snapshot read started before configuration cannot roll back the returned columns', async ({ page }) => {
  await nativeFixture(page);
  await hold(page, 'snapshot');
  await page.locator('.command-refresh').click();
  await pending(page, 'snapshot');
  await open(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  await release(page, 'snapshot');
  await card(page).click({ button: 'right' });
  await page.getByRole('menuitem', { name: '移动到…', exact: true }).click();
  await expect(page.getByRole('menu', { name: '移动到列' }).getByRole('menuitem')).toHaveCount(3);
});

test('a delayed post-write snapshot cannot overwrite a newer watcher permission snapshot', async ({ page }) => {
  await nativeFixture(page);
  await open(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await hold(page, 'snapshot');
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  // This is acceptWrite's read after the configuration was applied, not a
  // pre-write refresh. A later watcher sees the permission revoked on disk.
  await pending(page, 'snapshot');
  await page.evaluate(() => window.__nativeColumns.setWritable(false));
  await expect(dialog(page)).toContainText('只读');
  const task = card(page);
  await expect(task.getByRole('button', { name: `完成“${taskName}”`, exact: true, includeHidden: true })).toBeDisabled();
  await release(page, 'snapshot');
  await expect(dialog(page)).toHaveCount(0);
  await expect(task.getByRole('button', { name: `完成“${taskName}”`, exact: true })).toBeDisabled();
  await expect(page.locator('.revision-chip')).toHaveAttribute('title', 'Snapshot revision permission-2');
  expect(await calls(page, 'apply_columns')).toHaveLength(1);
});

test('revoking source write permission invalidates an already displayed preview', async ({ page }) => {
  await nativeFixture(page);
  await open(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toBeEnabled();
  await page.evaluate(() => window.__nativeColumns.setWritable(false));
  await expect(dialog(page)).toContainText('只读');
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
  await expect(dialog(page).getByRole('button', { name: '预览差异', exact: true })).toBeDisabled();
  expect(await calls(page, 'apply_columns')).toHaveLength(0);
});
