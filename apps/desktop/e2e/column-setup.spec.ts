import { expect, test, type Page, type Route } from '@playwright/test';

const root = '/fixtures/column-setup';
const source = 'docs/TODO.md';
const otherSource = 'notes/work.md';
type Selection = { mode: 'defaults' } | { mode: 'defaults_and_start'; task_id: string } | { mode: 'existing'; headings: Array<{ name: string; done: boolean }> };
type ApplyBody = { source_path: string; expected_project_path: string; base_hash: string; selection: Selection };

const headings = [
  { name: '一、优先级', line: 5, selectable: true, reason: null },
  { name: '已完成', line: 11, selectable: true, reason: null },
  { name: '重复', line: 15, selectable: false, reason: '存在同名二级标题，无法唯一定位。' },
  { name: '重复', line: 18, selectable: false, reason: '存在同名二级标题，无法唯一定位。' },
];

function task(source_path: string, body: string, column: string | null = null) {
  return {
    source_path, id: 'same01', parent_id: null, body, completed: false, column, indent_level: 0,
    due: null, priority: 2, tags: [], progress: null, failed_on: null, note: '保留原任务说明', evidence: null,
    evidence_availability: { status: 'unavailable', reason: '测试文件没有 Git 物证。' },
  };
}

async function fixture(page: Page, options: {
  readOnly?: boolean; writable?: boolean; taskWritable?: boolean; optionsErrorOnce?: boolean;
  conflictOnce?: boolean; configured?: boolean; ownColumns?: 'active' | 'done-only'; sourceBody?: string;
} = {}) {
  const paths = ['TODO.md', source, otherSource];
  await page.addInitScript(({ root, paths }) => {
    localStorage.setItem(`todo.source-enrollment.v1:${root}`, JSON.stringify({ enrolled: paths, seen: paths }));
  }, { root, paths });
  const tasks = [task('TODO.md', '已有列任务', 'todo'), task(source, options.sourceBody ?? '源文档任务'), task(otherSource, '第二来源任务')]
    .map((item) => ({ ...item, source_writable: item.source_path !== source || options.taskWritable !== false }));
  const sources = paths.map((path) => ({
    path, status: 'managed', writable: path !== source || options.writable !== false,
    revision: 'source-original', task_count: 1, preview: '# 原始文档', completion_policy: 'normal',
  }));
  const columns = [{ source_path: 'TODO.md', id: 'todo', name: '别的来源已有列', done: false }];
  if (options.ownColumns) columns.push({ source_path: source, id: options.ownColumns === 'active' ? 'doing' : 'done', name: options.ownColumns === 'active' ? '本来源推进中' : '已完成', done: options.ownColumns === 'done-only' });
  const calls = {
    moves: [] as Array<{ source_path: string; intent: { id: string; column: string; type: string } }>,
    options: [] as URL[],
    previews: [] as Array<{ source_path: string; expected_project_path: string; selection: Selection }>,
    applies: [] as ApplyBody[],
    unexpected: [] as string[],
  };
  let revision = 0;
  let hash = 'file-hash-original';
  let failOptions = options.optionsErrorOnce;
  let conflict = options.conflictOnce;
  const holds = new Map<string, { started: () => void; wait: Promise<void> }>();
  const holdNext = (kind: 'options' | 'preview' | 'apply' | 'snapshot') => {
    let started!: () => void;
    let release!: () => void;
    const start = new Promise<void>((resolve) => { started = resolve; });
    const wait = new Promise<void>((resolve) => { release = resolve; });
    holds.set(kind, { started, wait });
    return { started: start, release };
  };
  const reply = async (route: Route, kind: string, json: unknown) => {
    const hold = holds.get(kind);
    holds.delete(kind);
    if (hold) { hold.started(); await hold.wait; }
    await route.fulfill({ json });
  };
  const snapshot = (selected: string | null = null) => ({
    project_path: root, revision: `project-${revision}`, completion_policy: 'normal',
    sources, selected_source: selected, warnings: [],
    columns: columns.filter((item) => selected === null || item.source_path === selected),
    tasks: tasks.filter((item) => selected === null || item.source_path === selected),
    today: { overdue: [], today: [], upcoming: [], next: tasks.filter((item) => selected === null || item.source_path === selected) },
  });
  await page.route('**/api/v1/**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname.replace('/api/v1', '');
    if (path === '/health') return route.fulfill({ json: { ok: true, read_only: options.readOnly === true, project: root } });
    if (path === '/snapshot') return reply(route, 'snapshot', snapshot(url.searchParams.get('source')));
    if (path === '/ai/sessions') return route.fulfill({ json: { records: [], connected: false } });
    if (path === '/sources/columns' && request.method() === 'GET') {
      calls.options.push(url);
      if (failOptions) {
        failOptions = false;
        return route.fulfill({ status: 500, json: { error: '临时读取失败' } });
      }
      return reply(route, 'options', { source_path: url.searchParams.get('source'), base_hash: hash, configured: options.configured === true, headings });
    }
    if (path === '/sources/columns/preview') {
      const body = request.postDataJSON();
      calls.previews.push(body);
      const selection = body.selection as Selection;
      const planned = selection.mode !== 'existing'
        ? [{ id: 'todo', name: '待办', done: false, created: true }, { id: 'doing', name: '进行中', done: false, created: true }, { id: 'done', name: '已完成', done: true, created: false }]
        : selection.headings.map((heading, index) => ({ id: `column-${index + 1}`, ...heading, created: false }));
      return reply(route, 'preview', {
        source_path: body.source_path, base_hash: hash, selection, columns: planned,
        proposed: 'SERVER GENERATED DOCUMENT — must never be submitted',
        diff: {
          added: 1, removed: 0,
          hunks: [
            ...Array.from({ length: 12 }, (_, index) => ({ kind: 'context', left_line: index + 1, right_line: index + 1, text: index === 0 ? '保留的开头原文' : `原文第 ${index + 1} 行` })),
            { kind: 'added', left_line: null, right_line: 13, text: `columns: ${planned.map((column) => column.name).join(', ')}` },
          ],
        },
      });
    }
    if (path === '/sources/columns' && request.method() === 'POST') {
      const body = request.postDataJSON() as ApplyBody;
      calls.applies.push(body);
      if (conflict) {
        conflict = false;
        hash = 'file-hash-after-external-edit';
        return route.fulfill({ status: 409, json: { error: '文件已在外部更新，未写入' } });
      }
      expect(body.base_hash).toBe(hash);
      const selected = body.selection.mode !== 'existing'
        ? [{ id: 'todo', name: '待办', done: false }, { id: 'doing', name: '进行中', done: false }, { id: 'done', name: '已完成', done: true }]
        : body.selection.headings.map((heading, index) => ({ id: `column-${index + 1}`, ...heading }));
      columns.push(...selected.map((column) => ({ ...column, source_path: body.source_path })));
      if (body.selection.mode === 'defaults_and_start') {
        const id = body.selection.task_id;
        const selectedTask = tasks.find((item) => item.source_path === body.source_path && item.id === id);
        expect(selectedTask?.column).toBeNull();
        if (selectedTask) selectedTask.column = 'doing';
      }
      revision += 1;
      return reply(route, 'apply', snapshot());
    }
    if (path === '/mutate') {
      const body = request.postDataJSON();
      calls.moves.push(body);
      const selectedTask = tasks.find((item) => item.source_path === body.source_path && item.id === body.intent.id);
      if (selectedTask) selectedTask.column = body.intent.column;
      revision += 1;
      return route.fulfill({ json: snapshot() });
    }
    calls.unexpected.push(`${request.method()} ${path}`);
    return route.fulfill({ status: 404, json: { error: 'Unexpected route in column setup fixture' } });
  });
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toBeVisible();
  return { calls, tasks, sources, columns, holdNext };
}

const card = (page: Page, body = '源文档任务') => page.getByRole('article', { name: `未完成 Project Task：${body}`, exact: true });
const dialog = (page: Page) => page.getByRole('dialog', { name: '配置任务列', exact: true });
async function openSetup(page: Page, body = '源文档任务') {
  await card(page, body).click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: '移动到…', exact: true })).toBeDisabled();
  await page.getByRole('menuitem', { name: '配置任务列…', exact: true }).click();
  await expect(dialog(page)).toBeVisible();
}

async function flush(page: Page) {
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
}

test('dragging an unclassified task to focus previews one source-bound create-and-start operation', async ({ page }) => {
  const { calls, tasks, columns } = await fixture(page);
  const original = structuredClone(tasks);
  // The other source is already in focus; show two cards without changing its
  // position so this test can inspect both source-owned tasks after the drop.
  await page.getByRole('button', { name: '增加同时显示数', exact: true }).click();
  const target = page.locator('.goal-region--doing');
  const from = (await card(page).boundingBox())!;
  const to = (await target.boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 12 });
  await page.mouse.move(to.x + to.width / 2 + 1, to.y + to.height / 2 + 1);
  await expect(target).toHaveClass(/is-drop-target/);
  await page.mouse.up();
  const start = page.getByRole('dialog', { name: '创建列并推进任务', exact: true });
  await expect(start).toBeVisible();
  await expect(start.getByRole('table', { name: '左右对比' })).toBeVisible();
  await expect(start).toContainText('源文档任务');
  await expect(start).toContainText(source);
  await expect(start.getByRole('radio')).toHaveCount(0);
  expect(calls.previews[0]).toEqual({ source_path: source, expected_project_path: root, selection: { mode: 'defaults_and_start', task_id: 'same01' } });
  expect(calls.moves).toEqual([]);
  expect(calls.applies).toEqual([]);
  await start.getByRole('button', { name: '取消', exact: true }).click();
  await expect(start).toHaveCount(0);
  expect(tasks).toEqual(original);
  expect(columns.filter((item) => item.source_path === source)).toEqual([]);
  await card(page).dragTo(target);
  await start.getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(start).toHaveCount(0);
  await expect(target.getByRole('article', { name: '未完成 Project Task：源文档任务', exact: true })).toBeVisible();
  expect(calls.applies).toEqual([{ source_path: source, expected_project_path: root, base_hash: 'file-hash-original', selection: { mode: 'defaults_and_start', task_id: 'same01' } }]);
  expect(tasks.find((item) => item.source_path === otherSource)).toEqual(original.find((item) => item.source_path === otherSource));
  expect(calls.moves).toEqual([]);
});

test('unclassified task can start by keyboard menu and conflicts require repreview', async ({ page }) => {
  const { calls } = await fixture(page, { conflictOnce: true });
  await card(page).focus();
  await card(page).press('Shift+F10');
  await page.getByRole('menuitem', { name: '开始处理', exact: true }).press('Enter');
  const start = page.getByRole('dialog', { name: '创建列并推进任务', exact: true });
  await start.getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(start.getByRole('alert')).toContainText('重新预览');
  await expect(start.getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
  await start.getByRole('button', { name: '重新预览差异', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(start).toHaveCount(0);
  expect(calls.applies).toHaveLength(1);
  expect(calls.previews.every((call) => call.selection.mode === 'defaults_and_start')).toBe(true);
});

test('dragging to focus uses the task own existing focus column, never another source', async ({ page }) => {
  const { calls } = await fixture(page, { ownColumns: 'active' });
  await card(page).dragTo(page.locator('.goal-region--doing'));
  await expect.poll(() => calls.moves.length).toBe(1);
  expect(calls.moves[0]).toMatchObject({ source_path: source, intent: { type: 'move_task', id: 'same01', column: 'doing' } });
  expect(calls.previews).toEqual([]);
  expect(calls.applies).toEqual([]);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('a source with only done columns explains why it cannot start without overwriting configuration', async ({ page }) => {
  const { calls } = await fixture(page, { ownColumns: 'done-only' });
  await card(page).click({ button: 'right' });
  await expect(page.getByText('此来源没有可用的推进列，不能开始处理；不会覆盖已有列。', { exact: true })).toBeVisible();
  await expect(page.getByRole('menuitem', { name: '开始处理', exact: true })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await card(page).dragTo(page.locator('.goal-region--doing'));
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(calls.previews).toEqual([]);
  expect(calls.moves).toEqual([]);
});

test('defaults preview is source-bound in aggregate view and confirms only typed intent', async ({ page }) => {
  const { calls, tasks } = await fixture(page);
  const beforeTasks = structuredClone(tasks);
  await openSetup(page);
  await expect(dialog(page)).toContainText(source);
  await expect(dialog(page)).toContainText(root);
  await expect(dialog(page).getByRole('radio', { name: '创建默认状态列（推荐）', exact: true })).toBeChecked();
  await expect(dialog(page)).toContainText('不移动或重排现有任务');
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await expect(dialog(page).getByRole('table', { name: '左右对比' })).toContainText('保留的开头原文');
  await expect(dialog(page).getByRole('list', { name: '将配置的列' })).toContainText('新建');
  await expect(dialog(page).getByRole('list', { name: '将配置的列' })).toContainText('复用');
  expect(calls.applies).toHaveLength(0);
  expect(calls.options[0].searchParams.get('source')).toBe(source);
  expect(calls.options[0].searchParams.get('expected_project_path')).toBe(root);
  expect(calls.previews).toEqual([{ source_path: source, expected_project_path: root, selection: { mode: 'defaults' } }]);
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(calls.applies).toEqual([{ source_path: source, expected_project_path: root, base_hash: 'file-hash-original', selection: { mode: 'defaults' } }]);
  expect(tasks).toEqual(beforeTasks);
  await card(page).click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: '配置任务列…', exact: true })).toHaveCount(0);
  await page.getByRole('menuitem', { name: '移动到…', exact: true }).click();
  await expect(page.getByRole('menu', { name: '移动到列', exact: true }).getByRole('menuitem')).toHaveText(['待办', '进行中', '已完成']);
  expect(calls.unexpected).toEqual([]);
});

test('existing headings require explicit checks and never infer done from the title', async ({ page }) => {
  const { calls } = await fixture(page);
  await openSetup(page);
  await dialog(page).getByRole('radio', { name: '使用现有标题', exact: true }).check();
  const preview = dialog(page).getByRole('button', { name: '预览差异', exact: true });
  await expect(preview).toBeDisabled();
  const duplicates = dialog(page).getByRole('checkbox', { name: /^使用「重复」/ });
  await expect(duplicates).toHaveCount(2);
  for (const checkbox of await duplicates.all()) await expect(checkbox).toBeDisabled();
  await expect(dialog(page)).toContainText('无法唯一定位');
  await dialog(page).getByRole('checkbox', { name: /^使用「已完成」/ }).check();
  const done = dialog(page).getByRole('checkbox', { name: '将「已完成」设为完成列', exact: true });
  await expect(done).not.toBeChecked();
  await preview.click();
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toBeEnabled();
  expect(calls.previews[0].selection).toEqual({ mode: 'existing', headings: [{ name: '已完成', done: false }] });
  await done.check();
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
  await dialog(page).getByRole('checkbox', { name: /^使用「一、优先级」/ }).check();
  await preview.click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(calls.applies[0].selection).toEqual({ mode: 'existing', headings: [{ name: '一、优先级', done: false }, { name: '已完成', done: true }] });
  expect(calls.applies[0]).not.toHaveProperty('proposed');
});

test('cancel, Escape and return to selection never write and restore the task focus', async ({ page }) => {
  const { calls } = await fixture(page);
  await card(page).focus();
  await card(page).press('Shift+F10');
  await page.getByRole('menuitem', { name: '配置任务列…', exact: true }).click();
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '返回选择', exact: true }).click();
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
  await dialog(page).getByRole('button', { name: '取消', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(card(page)).toBeFocused();
  await openSetup(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await expect(dialog(page).getByRole('table', { name: '左右对比' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog(page)).toHaveCount(0);
  expect(calls.applies).toEqual([]);
});

test('conflict invalidates confirmation and requires a fresh backend preview', async ({ page }) => {
  const { calls } = await fixture(page, { conflictOnce: true });
  await openSetup(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page).getByRole('alert')).toContainText('重新预览');
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
  expect(calls.applies).toHaveLength(1);
  await dialog(page).getByRole('button', { name: '重新预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(calls.previews).toHaveLength(2);
  expect(calls.applies[1].base_hash).toBe('file-hash-after-external-edit');
});

test('late preview after changing choices cannot revive its confirm button', async ({ page }) => {
  const { calls, holdNext } = await fixture(page);
  await openSetup(page);
  const held = holdNext('preview');
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await held.started;
  await dialog(page).getByRole('radio', { name: '使用现有标题', exact: true }).check();
  held.release();
  await flush(page);
  await expect(dialog(page).getByRole('button', { name: '确认并写入', exact: true })).toHaveCount(0);
  await dialog(page).getByRole('checkbox', { name: /^使用「一、优先级」/ }).check();
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await expect(dialog(page).getByRole('list', { name: '将配置的列' })).toContainText('一、优先级');
  expect(calls.applies).toEqual([]);
});

test('late options from a canceled source cannot replace a newly opened source dialog', async ({ page }) => {
  const { calls, holdNext } = await fixture(page);
  const held = holdNext('options');
  await openSetup(page);
  await held.started;
  await page.keyboard.press('Escape');
  await page.getByRole('combobox', { name: '任务来源', exact: true }).selectOption(otherSource);
  await openSetup(page, '第二来源任务');
  await expect(dialog(page).getByRole('button', { name: '预览差异', exact: true })).toBeEnabled();
  held.release();
  await flush(page);
  await expect(dialog(page)).toContainText(otherSource);
  await expect(dialog(page)).not.toContainText(source);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(calls.applies[0].source_path).toBe(otherSource);
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toHaveValue(otherSource);
});

test('an in-flight confirmation cannot be canceled or submitted twice', async ({ page }) => {
  const { calls, holdNext } = await fixture(page);
  await openSetup(page);
  await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
  const held = holdNext('apply');
  await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
  await held.started;
  await expect(dialog(page).getByRole('button', { name: '写入中…', exact: true })).toBeDisabled();
  await expect(dialog(page).getByRole('button', { name: '取消', exact: true })).toBeDisabled();
  await expect(dialog(page).getByRole('radio', { name: '使用现有标题', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await page.keyboard.press('Enter');
  await expect(dialog(page)).toBeVisible();
  expect(calls.applies).toHaveLength(1);
  held.release();
  await expect(dialog(page)).toHaveCount(0);
  expect(calls.applies).toHaveLength(1);
});

for (const viewport of [{ width: 986, height: 320 }, { width: 320, height: 568 }]) {
  test(`configuration stays keyboard-accessible with both diff sides at ${viewport.width}×${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await fixture(page);
    await openSetup(page);
    await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
    const confirm = dialog(page).getByRole('button', { name: '确认并写入', exact: true });
    const cancel = dialog(page).getByRole('button', { name: '取消', exact: true });
    const diff = dialog(page).getByRole('table', { name: '左右对比' });
    await expect(diff.getByRole('columnheader', { name: '配置后', exact: true })).toBeVisible();
    await expect(diff.locator('.diff-cell--added').first()).toBeVisible();
    for (const element of [dialog(page), confirm, cancel]) {
      const box = (await element.boundingBox())!;
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
      expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
    }
    expect(await dialog(page).evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
    for (let index = 0; index < 12; index += 1) {
      await page.keyboard.press('Tab');
      const focus = await dialog(page).evaluate((element) => ({
        inside: element.contains(document.activeElement),
        active: document.activeElement?.outerHTML.slice(0, 240),
      }));
      expect(focus.inside, `Tab ${index + 1}: ${focus.active}`).toBe(true);
    }
    for (let index = 0; index < 12; index += 1) {
      await page.keyboard.press('Shift+Tab');
      expect(await dialog(page).evaluate((element) => element.contains(document.activeElement))).toBe(true);
    }
    await test.info().attach(`column-setup-${viewport.width}`, { body: await dialog(page).screenshot({ path: test.info().outputPath(`column-setup-${viewport.width}.png`), animations: 'disabled' }), contentType: 'image/png' });
    await cancel.focus();
    await page.keyboard.press('Enter');
    await expect(dialog(page)).toHaveCount(0);
    expect(errors).toEqual([]);
  });
}

test('loading failures can retry and preconfigured sources cannot be overwritten', async ({ page }) => {
  const { calls } = await fixture(page, { optionsErrorOnce: true, configured: true });
  await openSetup(page);
  await expect(dialog(page).getByRole('alert')).toContainText('临时读取失败');
  await dialog(page).getByRole('button', { name: '重新读取', exact: true }).click();
  await expect(dialog(page)).toContainText('此来源已定义任务列');
  await expect(dialog(page).getByRole('button', { name: '预览差异', exact: true })).toBeDisabled();
  expect(calls.previews).toEqual([]);
  expect(calls.applies).toEqual([]);
});

for (const options of [{ readOnly: true }, { writable: false }, { taskWritable: false }]) {
  test(`readonly task cannot offer configuration or write (${JSON.stringify(options)})`, async ({ page }) => {
    const { calls } = await fixture(page, options);
    await card(page).click({ button: 'right' });
    await expect(page.getByRole('menuitem', { name: '配置任务列…', exact: true })).toHaveCount(0);
    await expect(page.getByRole('menuitem', { name: '开始处理', exact: true })).toHaveCount(0);
    await expect(card(page)).toHaveAttribute('draggable', 'false');
    await expect(card(page).getByRole('button', { name: '完成“源文档任务”', exact: true })).toBeDisabled();
    await expect(card(page).getByRole('button', { name: '打开“源文档任务”的任务菜单', exact: true })).toHaveCount(0);
    expect(calls.applies).toEqual([]);
    expect(calls.previews).toEqual([]);
  });
}

test('the task being started renders its Markdown and its menu header is plain text', async ({ page }) => {
  await fixture(page, { sourceBody: '**未完成 · 库存释放**：`ReleaseReserve` 仍为 panic' });
  const plain = '未完成 · 库存释放：ReleaseReserve 仍为 panic';
  await card(page, plain).focus();
  await card(page, plain).press('Shift+F10');
  const menu = page.getByRole('menu', { name: `“${plain}”的任务菜单` });
  await expect(menu.locator('.task-context-menu__task')).toHaveText(plain);
  await page.getByRole('menuitem', { name: '开始处理', exact: true }).press('Enter');
  const start = page.getByRole('dialog', { name: '创建列并推进任务', exact: true });
  const quoted = start.locator('.column-setup-dialog__task');
  await expect(quoted.locator('strong')).toHaveText('未完成 · 库存释放');
  await expect(quoted.locator('code')).toHaveText('ReleaseReserve');
  await expect(quoted).not.toContainText('**');
});
