import { expect, test, type Page } from '@playwright/test';

const task = (source_path: string, body: string, id = 'same01') => ({
  source_path, id, parent_id: null, body, completed: false, column: 'todo', indent_level: 0,
  due: null, priority: 2, tags: [], progress: null, failed_on: null, note: null, evidence: null,
  evidence_availability: { status: 'unavailable', reason: '测试来源没有物证' },
});

async function project(page: Page, options: { readonly?: boolean; conflict?: boolean; enrollment?: 'all' | 'none' } = {}) {
  const paths = ['TODO.md', 'nested/work.md', 'notes/plain.md', 'broken.md'];
  if (options.enrollment !== 'none') {
    // Most specs are about writes and adoption, not the first-run enrollment
    // sheet; they start from a project where every source is already enrolled.
    await page.addInitScript((value: string) => {
      // Seed once; a reload must keep whatever the test changed since.
      const key = 'todo.source-enrollment.v1:/test/project-root';
      if (!localStorage.getItem(key)) localStorage.setItem(key, value);
    }, JSON.stringify({ enrolled: paths, seen: paths }));
  }
  const sources = paths.map((path, index) => ({
    path, status: index < 2 ? 'managed' : index === 2 ? 'unmanaged' : 'error',
    writable: index < 2, revision: `source-${index}`, task_count: index === 3 ? 0 : 1,
    preview: index === 3 ? '---\nbroken frontmatter: [' : '- [ ] 普通清单原文',
    error: index === 3 ? 'frontmatter parse error' : null, completion_policy: 'normal', loose_candidates: 0 as number,
  }));
  const tasks = [task(paths[0], '来源甲任务'), task(paths[1], '来源乙任务'), task(paths[2], '未接管任务', 'loose1')];
  const columns = paths.slice(0, 3).flatMap((source_path) => ['todo', 'doing', 'done'].map((id) => ({ source_path, id, name: id === 'todo' ? '待办' : id === 'doing' ? '推进中' : '已完成', done: id === 'done' })));
  let revision = 1;
  const mutations: Array<{ source_path: string; intent: Record<string, unknown> }> = [];
  const previews: unknown[] = [];
  const adopts: unknown[] = [];
  const unexpected: string[] = [];
  const snapshot = (selected: string | null = null, loose: string[] = []) => ({
    // Like the real backend, the revision hashes the source list too.
    project_path: '/test/project-root', revision: `r${revision}-${sources.length}-${loose.join(',')}`, completion_policy: 'normal',
    sources: sources.map((source) => loose.includes(source.path)
      ? { ...source, loose: true, status: 'unmanaged', writable: false, task_count: source.loose_candidates ?? 0 }
      : source),
    selected_source: selected, warnings: ['普通清单仅作只读发现'],
    columns: columns.filter((item) => selected === null || item.source_path === selected),
    tasks: [
      ...tasks.filter((item) => selected === null || item.source_path === selected),
      ...sources.filter((source) => loose.includes(source.path) && (selected === null || selected === source.path))
        .flatMap((source) => Array.from({ length: source.loose_candidates ?? 0 }, (_, index) => ({ ...task(source.path, `列表项${index + 1}`, `loose${index}`), column: null, source_writable: false }))),
    ],
    today: { overdue: [], today: [], upcoming: [], next: tasks.filter((item) => selected === null || item.source_path === selected) },
  });
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    if (path === '/health') return route.fulfill({ json: { ok: true, read_only: options.readonly === true, project: '/test/project-root', captures: false } });
    if (path === '/snapshot') return route.fulfill({ json: snapshot(url.searchParams.get('source'), (url.searchParams.get('loose') ?? '').split('\n').filter(Boolean)) });
    if (path === '/mutate') {
      const body = route.request().postDataJSON(); mutations.push(body);
      const own = tasks.find((item) => item.id === body.intent.id && item.source_path === body.source_path);
      if (body.intent.type === 'set_priority' && own) own.priority = body.intent.priority;
      if (body.intent.type === 'add_task') tasks.push({ ...task(body.source_path, body.intent.body, 'new001'), column: body.intent.column });
      if (body.intent.type === 'move_task' && own) own.column = body.intent.column;
      revision += 1;
      return route.fulfill({ json: snapshot() });
    }
    if (path === '/sources/adopt/preview') {
      const body = route.request().postDataJSON(); previews.push(body);
      if (body.loose) return route.fulfill({ json: { source_path: body.source_path, base_hash: 'loose-hash', proposed: 'converted', diff: { added: 3, removed: 3, hunks: [{ kind: 'removed', left_line: 1, right_line: null, text: '- 列表项1' }, { kind: 'added', left_line: null, right_line: 1, text: '- [ ] 列表项1 <!--t:aaaaaa-->' }] } } });
      return route.fulfill({ json: { source_path: body.source_path, base_hash: 'preview-hash', proposed: 'managed document', diff: { added: 1, removed: 0, hunks: [{ kind: 'added', left_line: null, right_line: 1, text: 'todo-spec: 1' }] } } });
    }
    if (path === '/sources/adopt') {
      const body = route.request().postDataJSON(); adopts.push(body);
      if (options.conflict) return route.fulfill({ status: 409, json: { error: '文件已在外部更新，未写入' } });
      const target = sources.find((source) => source.path === body.source_path) ?? sources[2];
      target.status = 'managed'; target.writable = true;
      if (body.loose) { target.task_count = target.loose_candidates ?? 0; target.loose_candidates = 0; }
      revision += 1;
      return route.fulfill({ json: snapshot() });
    }
    unexpected.push(`${route.request().method()} ${path}`);
    return route.fulfill({ status: 404, json: { error: 'not available in this fixture' } });
  });
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toBeVisible();
  return { mutations, previews, adopts, unexpected, snapshot, sources };
}

const card = (page: Page, body: string) => page.getByRole('article', { name: `未完成 Project Task：${body}`, exact: true });

test('同 local ID 的多来源任务显示来源且只修改目标文件', async ({ page }) => {
  const { mutations } = await project(page);
  const a = card(page, '来源甲任务');
  const b = card(page, '来源乙任务');
  await expect(a.locator('.task-source')).toHaveText('TODO.md');
  await expect(b.locator('.task-source')).toHaveText('nested/work.md');
  expect(await a.getAttribute('data-task-id')).not.toBe(await b.getAttribute('data-task-id'));
  await expect(a.locator('.task-id')).toHaveText('same01');
  await b.focus();
  await b.press('1');
  await expect.poll(() => mutations.length).toBe(1);
  expect(mutations[0]).toMatchObject({ source_path: 'nested/work.md', intent: { type: 'set_priority', id: 'same01', priority: 1 } });
  await expect(b.getByLabel('优先级 1')).toBeVisible();
  await expect(a.getByLabel('优先级 2')).toBeVisible();
});

test('新增必须明确选源，来源选择记忆但不是偷偷选第一个', async ({ page }) => {
  const { mutations } = await project(page);
  const source = page.getByRole('combobox', { name: '新增到来源', exact: true });
  await expect(source).toHaveValue('');
  await page.getByRole('textbox', { name: '快速新增 Project Task' }).fill('明确选源的新任务');
  const add = page.getByRole('button', { name: '添加', exact: true });
  await expect(add).toBeDisabled();
  await source.selectOption('nested/work.md');
  await expect(add).toBeEnabled();
  await add.click();
  await expect.poll(() => mutations.length).toBe(1);
  expect(mutations[0]).toMatchObject({ source_path: 'nested/work.md', intent: { type: 'add_task', column: 'todo', body: '明确选源的新任务' } });
  await page.reload();
  await expect(page.getByRole('combobox', { name: '新增到来源' })).toHaveValue('nested/work.md');
});

test('跨文件移动拒绝且菜单不列出其他来源列', async ({ page }) => {
  const { mutations } = await project(page);
  const a = card(page, '来源甲任务');
  const bLane = page.locator('.todo-lane').filter({ hasText: 'nested/work.md' });
  await a.dragTo(bLane);
  await expect(page.getByRole('alert')).toContainText('不能跨来源文件');
  expect(mutations).toHaveLength(0);
  await a.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '移动到…' }).click();
  await expect(page.getByRole('menu', { name: '移动到列' }).getByRole('menuitem')).toHaveCount(3);
});

test('未管理任务只读，选源显示原文并要求两步接管', async ({ page }) => {
  const { previews, adopts } = await project(page);
  const loose = card(page, '未接管任务');
  await expect(loose.getByRole('button', { name: '完成“未接管任务”', exact: true })).toBeDisabled();
  await expect(loose).toHaveAttribute('draggable', 'false');
  await page.getByRole('combobox', { name: '任务来源', exact: true }).selectOption('notes/plain.md');
  await expect(page.getByLabel('来源原文')).toContainText('- [ ] 普通清单原文');
  expect(adopts).toHaveLength(0);
  await page.getByRole('button', { name: '预览接管差异' }).click();
  await expect(page.getByRole('table', { name: '左右对比' })).toContainText('todo-spec: 1');
  expect(previews).toEqual([{ source_path: 'notes/plain.md', loose: false }]);
  expect(adopts).toHaveLength(0);
  await page.getByRole('button', { name: '取消接管' }).click();
  expect(adopts).toHaveLength(0);
  await page.getByRole('button', { name: '预览接管差异' }).click();
  await page.getByRole('button', { name: '确认接管并写入' }).click();
  await expect.poll(() => adopts.length).toBe(1);
  expect(adopts[0]).toEqual({ source_path: 'notes/plain.md', base_hash: 'preview-hash', loose: false });
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toHaveValue('notes/plain.md');
  await expect(card(page, '未接管任务').getByRole('button', { name: '完成“未接管任务”', exact: true })).toBeEnabled();
});

test('接管冲突不改变当前来源且清除旧确认', async ({ page }) => {
  const { adopts } = await project(page, { conflict: true });
  await page.getByRole('combobox', { name: '任务来源', exact: true }).selectOption('notes/plain.md');
  await page.getByRole('button', { name: '预览接管差异' }).click();
  await page.getByRole('button', { name: '确认接管并写入' }).click();
  await expect(page.getByLabel('来源只读预览').getByRole('alert')).toContainText('未写入');
  expect(adopts).toHaveLength(1);
  await expect(page.getByRole('button', { name: '确认接管并写入' })).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true })).toHaveValue('notes/plain.md');
});

test('只读服务不能接管；错误来源显示原文不提供写操作', async ({ page }) => {
  const { previews, adopts } = await project(page, { readonly: true });
  const source = page.getByRole('combobox', { name: '任务来源', exact: true });
  await source.selectOption('notes/plain.md');
  await expect(page.getByRole('button', { name: '预览接管差异' })).toBeDisabled();
  await source.selectOption('broken.md');
  await expect(page.getByLabel('来源原文')).toContainText('broken frontmatter');
  await expect(page.getByRole('button', { name: '预览接管差异' })).toHaveCount(0);
  expect(previews).toHaveLength(0); expect(adopts).toHaveLength(0);
});

test('聚合视图禁用来源依赖功能，HTTP随记明确不可用不mock', async ({ page }) => {
  const { unexpected } = await project(page);
  await page.getByRole('button', { name: '扫描与历史', exact: true }).click();
  await expect(page.getByText('扫描、历史与对齐提示词依赖单个来源。', { exact: false })).toBeVisible();
  await expect(page.getByRole('button', { name: '扫描', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '随记', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('HTTP 项目服务不开放个人随记');
  expect(unexpected).toEqual([]);
});

test('无列的已接管来源仍显示已完成并可取消完成', async ({ page }) => {
  let completed = true;
  const writes: unknown[] = [];
  const snapshot = () => ({ project_path: '/test/columnless', revision: String(completed), completion_policy: 'normal', selected_source: null,
    sources: [{ path: 'TODO.txt', status: 'managed', writable: true, revision: 'v1', task_count: 1, preview: '' }],
    columns: [], tasks: [{ ...task('TODO.txt', '已完成的无列任务'), completed, column: null }], today: { overdue: [], today: [], upcoming: [], next: [] } });
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/health')) return route.fulfill({ json: { ok: true, read_only: false, captures: false } });
    if (path.endsWith('/mutate')) { writes.push(route.request().postDataJSON()); completed = false; }
    return route.fulfill({ json: snapshot() });
  });
  await page.goto('/');
  await expect(page.getByRole('article', { name: '已完成 Project Task：已完成的无列任务', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '将“已完成的无列任务”标为未完成', exact: true }).click();
  await expect(card(page, '已完成的无列任务')).toBeVisible();
  expect(writes).toEqual([{ source_path: 'TODO.txt', intent: { type: 'set_completed', id: 'same01', completed: false } }]);
});

test('旧来源迟到刷新不得覆盖新选择', async ({ page }) => {
  const { snapshot } = await project(page);
  const select = page.getByRole('combobox', { name: '任务来源', exact: true });
  await select.selectOption('TODO.md');
  await expect(select).toHaveValue('TODO.md');
  let release!: () => void;
  const hold = new Promise<void>((resolve) => { release = resolve; });
  let reached = false;
  await page.route('**/api/v1/snapshot?source=TODO.md', async (route) => {
    reached = true;
    await hold;
    await route.fulfill({ json: snapshot('TODO.md') });
  });
  await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
  await expect.poll(() => reached).toBe(true);
  await select.selectOption('nested/work.md');
  await expect(select).toHaveValue('nested/work.md');
  release();
  await expect(card(page, '来源乙任务')).toBeVisible();
  await expect(card(page, '来源甲任务')).toHaveCount(0);
  await expect(select).toHaveValue('nested/work.md');
});

test('多来源首次打开先让人勾选录入哪些，未录入的来源不进看板且可随时调整', async ({ page }) => {
  await project(page, { enrollment: 'none' });
  const sheet = page.getByRole('dialog', { name: '录入任务来源' });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByRole('checkbox')).toHaveCount(4);
  await expect(sheet.getByText('已选 4 / 4')).toBeVisible();
  await sheet.getByRole('checkbox', { name: '录入 nested/work.md' }).uncheck();
  await sheet.getByRole('checkbox', { name: '录入 broken.md' }).uncheck();
  await expect(sheet.getByText('已选 2 / 4')).toBeVisible();
  await sheet.getByRole('button', { name: '录入所选（2）' }).click();
  await expect(sheet).toBeHidden();

  await expect(card(page, '来源甲任务')).toBeVisible();
  await expect(card(page, '来源乙任务')).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true }).getByRole('option')).toHaveCount(3);
  await expect(page.getByRole('combobox', { name: '新增到来源', exact: true }).getByRole('option', { name: 'nested/work.md' })).toHaveCount(0);
  await expect(page.getByText('已录入 2 / 4 个来源')).toBeVisible();

  await page.reload();
  await expect(page.getByRole('dialog', { name: '录入任务来源' })).toHaveCount(0);
  await expect(card(page, '来源乙任务')).toHaveCount(0);

  await page.getByRole('button', { name: '调整录入…' }).click();
  const adjust = page.getByRole('dialog', { name: '调整录入的来源' });
  await adjust.getByRole('checkbox', { name: '录入 nested/work.md' }).check();
  await adjust.getByRole('button', { name: '保存（3）' }).click();
  await expect(card(page, '来源乙任务')).toBeVisible();
});

test('录入后新发现的来源被标出，而不是悄悄进入看板', async ({ page }) => {
  const { sources } = await project(page);
  sources.push({ path: 'later/TODO.md', status: 'managed', writable: true, revision: 'source-late', task_count: 0, preview: '', error: null, completion_policy: 'normal' });
  await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
  const pending = page.getByRole('button', { name: '发现 1 个新来源，尚未录入' });
  await expect(pending).toBeVisible();
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true }).getByRole('option', { name: /later\/TODO\.md/ })).toHaveCount(0);
  await pending.click();
  const adjust = page.getByRole('dialog', { name: '调整录入的来源' });
  const row = adjust.getByRole('checkbox', { name: '录入 later/TODO.md' });
  await expect(row).not.toBeChecked();
  await expect(adjust.getByText('新发现')).toBeVisible();
  await row.check();
  await adjust.getByRole('button', { name: '保存（5）' }).click();
  await expect(pending).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: '任务来源', exact: true }).getByRole('option', { name: /later\/TODO\.md/ })).toHaveCount(1);
});

test('发现提示折叠在来源栏内，展开后逐条列出', async ({ page }) => {
  await project(page);
  const toggle = page.getByRole('button', { name: /发现提示/ });
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByRole('list', { name: '发现提示' })).toHaveCount(0);
  await toggle.click();
  await expect(page.getByRole('list', { name: '发现提示' })).toContainText('普通清单仅作只读发现');
  await toggle.click();
  await expect(page.getByRole('list', { name: '发现提示' })).toHaveCount(0);
});

test('批量接管：先看每个文件的差异，再逐个以预览指纹写入', async ({ page }) => {
  const { previews, adopts, sources } = await project(page);
  // A second unmanaged list, so the batch is more than one file.
  sources.push({ path: 'docs/todo/交易.md', status: 'unmanaged', writable: false, revision: 'source-5', task_count: 4, preview: '- [ ] 交易清单', error: null, completion_policy: 'normal', loose_candidates: 0 });
  await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
  await page.getByRole('button', { name: '发现 1 个新来源，尚未录入' }).click();
  const adjust = page.getByRole('dialog', { name: '调整录入的来源' });
  await adjust.getByRole('checkbox', { name: '录入 docs/todo/交易.md' }).check();
  await adjust.getByRole('button', { name: /保存/ }).click();

  await page.getByRole('button', { name: '接管 2 个未接管…' }).click();
  const dialog = page.getByRole('dialog', { name: '批量接管来源' });
  await expect(dialog.getByRole('checkbox')).toHaveCount(2);
  expect(previews).toHaveLength(0);
  await dialog.getByRole('button', { name: '预览差异（2）' }).click();
  await expect(dialog.getByText('共新增 2 行')).toBeVisible();
  expect(previews.map((item) => (item as { source_path: string }).source_path).sort()).toEqual(['docs/todo/交易.md', 'notes/plain.md']);
  expect(adopts).toHaveLength(0);

  await dialog.getByRole('button', { name: /notes\/plain\.md/ }).click();
  await expect(dialog.getByRole('table', { name: '左右对比' })).toContainText('todo-spec: 1');

  await dialog.getByRole('button', { name: '确认接管并写入（2）' }).click();
  await expect(dialog.getByText('已接管 2 个文件，全部可写。')).toBeVisible();
  // Files are adopted in discovery order, each with the hash it was previewed at.
  expect(adopts).toEqual([
    { source_path: 'notes/plain.md', base_hash: 'preview-hash', loose: false },
    { source_path: 'docs/todo/交易.md', base_hash: 'preview-hash', loose: false },
  ]);
  await dialog.getByRole('button', { name: '完成' }).click();
  await expect(page.getByRole('button', { name: /个未接管…/ })).toHaveCount(0);
});

test('批量接管遇到冲突即停止，后面的文件保持原样', async ({ page }) => {
  const { adopts, sources } = await project(page, { conflict: true });
  sources.push({ path: 'docs/todo/交易.md', status: 'unmanaged', writable: false, revision: 'source-5', task_count: 4, preview: '- [ ] 交易清单', error: null, completion_policy: 'normal', loose_candidates: 0 });
  await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
  await page.getByRole('button', { name: '发现 1 个新来源，尚未录入' }).click();
  await page.getByRole('dialog', { name: '调整录入的来源' }).getByRole('checkbox', { name: '录入 docs/todo/交易.md' }).check();
  await page.getByRole('dialog', { name: '调整录入的来源' }).getByRole('button', { name: /保存/ }).click();
  await page.getByRole('button', { name: '接管 2 个未接管…' }).click();
  const dialog = page.getByRole('dialog', { name: '批量接管来源' });
  await dialog.getByRole('button', { name: '预览差异（2）' }).click();
  await dialog.getByRole('button', { name: '确认接管并写入（2）' }).click();
  await expect(dialog.getByText(/已接管 0 个文件；在「notes\/plain\.md」停止/)).toBeVisible();
  expect(adopts).toHaveLength(1);
  await expect(dialog.getByText('失败')).toBeVisible();
});

test('宽松解析（Beta）默认关闭，没有复选框的文件才提示，开启后只读且可关闭，接管时转换为复选框', async ({ page }) => {
  const { previews, adopts, sources } = await project(page);
  sources.push({ path: 'notes/bullets.md', status: 'unmanaged', writable: false, revision: 'source-b', task_count: 0, preview: '- 列表项1\n- 列表项2\n- 列表项3', error: null, completion_policy: 'normal', loose_candidates: 3 });
  await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
  await page.getByRole('button', { name: '发现 1 个新来源，尚未录入' }).click();
  await page.getByRole('dialog', { name: '调整录入的来源' }).getByRole('checkbox', { name: '录入 notes/bullets.md' }).check();
  await page.getByRole('dialog', { name: '调整录入的来源' }).getByRole('button', { name: /保存/ }).click();
  const select = page.getByRole('combobox', { name: '任务来源', exact: true });

  // The file with real checkboxes never gets the offer.
  await select.selectOption('notes/plain.md');
  await expect(page.getByRole('button', { name: '启用宽松解析' })).toHaveCount(0);

  await select.selectOption('notes/bullets.md');
  await expect(page.getByText('这个文件里没有复选框，但有 3 个「- 」列表项。')).toBeVisible();
  await expect(card(page, '列表项1')).toHaveCount(0);
  await page.getByRole('button', { name: '启用宽松解析' }).click();
  await expect(page.getByText('宽松解析 · Beta')).toBeVisible();
  await expect(page.getByText(/宽松解析已开启.*共 3 项，只读/)).toBeVisible();
  await expect(select.locator('option[value="notes/bullets.md"]')).toHaveText(/宽松解析 Beta/);
  expect(adopts).toHaveLength(0);

  // Off again: nothing was written, the offer returns.
  await page.getByRole('button', { name: '关闭宽松解析' }).click();
  await expect(page.getByRole('button', { name: '启用宽松解析' })).toBeVisible();
  await page.getByRole('button', { name: '启用宽松解析' }).click();

  // The preference survives reload and aggregate view shows the read-only tasks.
  await page.reload();
  await expect(page.getByText('宽松解析 · Beta')).toBeVisible();
  await select.selectOption('');
  const loose = card(page, '列表项1');
  await expect(loose).toBeVisible();
  await expect(loose.getByRole('button', { name: '完成“列表项1”', exact: true })).toBeDisabled();

  // Adoption from loose mode sends loose:true and converts the bullets.
  await select.selectOption('notes/bullets.md');
  await page.getByRole('button', { name: '预览接管差异（转换 3 个列表项）' }).click();
  expect(previews.at(-1)).toEqual({ source_path: 'notes/bullets.md', loose: true });
  await expect(page.getByRole('table', { name: '左右对比' })).toContainText('- [ ] 列表项1');
  await page.getByRole('button', { name: '确认接管并写入' }).click();
  await expect.poll(() => adopts.length).toBe(1);
  expect(adopts[0]).toEqual({ source_path: 'notes/bullets.md', base_hash: 'loose-hash', loose: true });
  await expect(select.locator('option[value="notes/bullets.md"]')).toHaveText(/已管理/);
  expect(await page.evaluate(() => localStorage.getItem('todo.loose-sources.v1:/test/project-root'))).toBe('[]');
});

test('已管理但没有复选框的索引文件：空态说明原因并提供宽松解析入口', async ({ page }) => {
  const { sources } = await project(page);
  sources.push({ path: 'index/TODO.md', status: 'managed', writable: true, revision: 'source-i', task_count: 0, preview: '', error: null, completion_policy: 'normal', loose_candidates: 8 });
  await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
  await page.getByRole('button', { name: '发现 1 个新来源，尚未录入' }).click();
  const adjust = page.getByRole('dialog', { name: '调整录入的来源' });
  await adjust.getByRole('checkbox', { name: '录入 index/TODO.md' }).check();
  await adjust.getByRole('button', { name: /保存/ }).click();
  await page.getByRole('combobox', { name: '任务来源', exact: true }).selectOption('index/TODO.md');
  await expect(page.getByText('文件已读取，其中没有复选框')).toBeVisible();
  await expect(page.getByText(/index\/TODO\.md 已接管，但任务只从/)).toBeVisible();
  await page.getByRole('button', { name: '启用宽松解析' }).click();
  await expect(page.getByText('宽松解析 · Beta')).toBeVisible();
  await expect(page.getByText(/共 8 项，只读/)).toBeVisible();
});
