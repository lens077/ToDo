import { expect, test } from '@playwright/test';

/**
 * The Web端 against a live `todo serve`, with no mocks anywhere.
 *
 * `review.spec.ts` covers the interaction layer using fixtures; this covers the
 * part fixtures cannot: that the same React build, pointed at a real HTTP
 * backend, reads and writes an actual TODO.md.
 *
 * Skipped unless E2E_WEB_URL names a running server, so the default suite stays
 * hermetic. `scripts/e2e-web.sh` starts one and sets it.
 */
const webUrl = process.env.E2E_WEB_URL;
const fixturePath = process.env.E2E_TODO_PATH;
const restartSignal = process.env.E2E_RESTART_SIGNAL;
const restartReady = process.env.E2E_RESTART_READY;

async function restartServer(): Promise<void> {
  if (!restartSignal || !restartReady) throw new Error('restart signal paths are required');
  const { rm, writeFile } = await import('node:fs/promises');
  await rm(restartReady, { force: true });
  await writeFile(restartSignal, 'restart');
  await expect.poll(async () => {
    try {
      const response = await fetch(`${webUrl}/api/v1/health`);
      const ready = await import('node:fs/promises').then(({ access }) => access(restartReady).then(() => true, () => false));
      return response.ok && ready;
    } catch { return false; }
  }, { timeout: 15_000 }).toBe(true);
}

async function taskFile(): Promise<string> {
  if (!fixturePath) throw new Error('E2E_TODO_PATH is required for file assertions');
  const { readFile } = await import('node:fs/promises');
  return readFile(fixturePath, 'utf8');
}

/**
 * A served project aggregates several task files, so the UI addresses a task by
 * its source and stable id together, and writes that pair into the DOM as the
 * key built by `src/lib/taskSources.ts`. The mock suite never assigns a source,
 * so a bare id still matches there; only these specs, which run against a real
 * server, have to spell the pair out.
 */
function domKey(id: string, source = 'TODO.md'): string {
  return JSON.stringify([source, id]);
}

function groupRoot(id: string): string {
  return `[data-group-root='${domKey(id)}']`;
}

function taskNode(id: string): string {
  return `[data-task-id='${domKey(id)}']`;
}

/**
 * Scanning, history and the alignment prompt each act on one task file, so the
 * UI refuses them while the project is being browsed as an aggregate. Narrowing
 * the source filter is what a user does first, and these specs have to do the
 * same to reach those controls at all.
 */
async function selectSingleSource(page: import('@playwright/test').Page): Promise<void> {
  await page.getByRole('combobox', { name: '任务来源' }).selectOption({ label: 'TODO.md · 已管理' });
}

function taskLine(source: string, id: string): string {
  const line = source.split(/\r?\n/).find((candidate) => candidate.includes(`<!--t:${id}-->`));
  if (!line) throw new Error(`task ${id} is missing from fixture`);
  return line;
}

test.describe('web transport', () => {
  test.skip(!webUrl, 'set E2E_WEB_URL to a running `todo serve` to enable');
  // Every test exercises one live Git-backed repository, so concurrent writes
  // would make restore points and evidence ranges nondeterministic.
  test.describe.configure({ mode: 'serial' });

  test.use({ baseURL: webUrl });

  test('loads real tasks from the HTTP backend', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('navigation')).toBeVisible();
    // The fixture project has a task the mock data does not, so seeing it proves
    // the data came from the server rather than the built-in fixtures.
    await expect(page.getByText('给登录接口加限流')).toBeVisible();
  });

  test('adopted plain Markdown keeps priority usable and configures empty columns only after preview confirmation', async ({ page, request }) => {
    if (!fixturePath) throw new Error('E2E_TODO_PATH is required');
    const { writeFile, readFile, unlink } = await import('node:fs/promises');
    const { dirname, join } = await import('node:path');
    const planPath = join(dirname(fixturePath), 'docs/TODO.md');
    await writeFile(planPath, '# Plain tasks\n\n- [ ] 写迁移脚本\n', { flag: 'wx' });
    try {
      const previewResponse = await request.post('/api/v1/sources/adopt/preview', { data: { source_path: 'docs/TODO.md' } });
      expect(previewResponse.ok()).toBe(true);
      const preview = await previewResponse.json();
      const adoptedResponse = await request.post('/api/v1/sources/adopt', { data: { source_path: 'docs/TODO.md', base_hash: preview.base_hash } });
      expect(adoptedResponse.ok()).toBe(true);
      const adopted = await adoptedResponse.json();
      expect(adopted.columns.filter((column: { source_path: string }) => column.source_path === 'docs/TODO.md')).toHaveLength(0);
      await page.goto('/');
      await page.getByRole('button', { name: '全部录入', exact: true }).click();
      await page.getByRole('combobox', { name: '任务来源', exact: true }).selectOption('docs/TODO.md');
      await page.getByRole('button', { name: '象限', exact: true }).click();
      const task = page.getByRole('article', { name: '未完成 Project Task：写迁移脚本', exact: true });
      await task.click({ button: 'right' });
      await expect(page.getByRole('menuitem', { name: '移动到…', exact: true })).toBeDisabled();
      await expect(page.getByText('当前来源未定义列，无法移动任务。', { exact: true })).toBeVisible();
      await page.getByRole('menuitem', { name: '优先级…', exact: true }).click();
      await page.getByRole('menuitem', { name: '!1 最高' }).click();
      await expect(page.getByRole('region', { name: '!1 最高', exact: true }).getByRole('article', { name: '未完成 Project Task：写迁移脚本', exact: true })).toBeVisible();
      const snapshotResponse = await request.get('/api/v1/snapshot?source=docs%2FTODO.md');
      const snapshot = await snapshotResponse.json();
      expect(snapshot.tasks.find((item: { body: string }) => item.body === '写迁移脚本')).toMatchObject({ priority: 1, column: null });
      const beforeColumns = await readFile(planPath, 'utf8');
      const historyBefore = await (await request.get('/api/v1/history?source=docs%2FTODO.md')).json();
      const configuration = page.getByRole('dialog', { name: '配置任务列', exact: true });
      await task.click({ button: 'right' });
      await page.getByRole('menuitem', { name: '配置任务列…', exact: true }).click();
      await configuration.getByRole('button', { name: '预览差异', exact: true }).click();
      await expect(configuration.getByRole('table', { name: '左右对比' })).toContainText('columns:');
      expect(await readFile(planPath, 'utf8')).toBe(beforeColumns);
      await page.keyboard.press('Escape');
      await expect(configuration).toHaveCount(0);
      expect(await readFile(planPath, 'utf8')).toBe(beforeColumns);
      expect(await (await request.get('/api/v1/history?source=docs%2FTODO.md')).json()).toHaveLength(historyBefore.length);
      await task.click({ button: 'right' });
      await page.getByRole('menuitem', { name: '配置任务列…', exact: true }).click();
      await configuration.getByRole('button', { name: '预览差异', exact: true }).click();
      await configuration.getByRole('button', { name: '确认并写入', exact: true }).click();
      await expect(configuration).toHaveCount(0);
      const configured = await (await request.get('/api/v1/snapshot?source=docs%2FTODO.md')).json();
      expect(configured.columns.map((column: { id: string; name: string; done: boolean }) => [column.id, column.name, column.done]))
        .toEqual([['todo', '待办', false], ['doing', '进行中', false], ['done', '已完成', true]]);
      expect(configured.tasks).toEqual(snapshot.tasks);
      expect(await readFile(planPath, 'utf8')).toContain(beforeColumns.slice(beforeColumns.indexOf('# Plain tasks')));
      expect(await (await request.get('/api/v1/history?source=docs%2FTODO.md')).json()).toHaveLength(historyBefore.length + 1);
      await task.click({ button: 'right' });
      await page.getByRole('menuitem', { name: '移动到…', exact: true }).click();
      await expect(page.getByRole('menu', { name: '移动到列', exact: true }).getByRole('menuitem')).toHaveText(['待办', '进行中', '已完成']);
      await page.getByRole('menu', { name: '移动到列', exact: true }).getByRole('menuitem', { name: '进行中', exact: true }).click();
      await expect.poll(async () => {
        const moved = await (await request.get('/api/v1/snapshot?source=docs%2FTODO.md')).json();
        return moved.tasks.find((item: { body: string }) => item.body === '写迁移脚本')?.column;
      }).toBe('doing');
    } finally {
      await unlink(planPath);
    }
  });

  test('quadrant writes only priority tokens and survives a fresh page', async ({ page, browser }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();

    const before = await taskFile();
    const lowCard = page.getByRole('article', { name: /清理发布记录/ });
    const highest = page.getByRole('region', { name: '!1 最高' });
    await lowCard.dragTo(highest);
    await expect(highest.getByText('清理发布记录')).toBeVisible();
    const afterMove = await taskFile();
    expect(taskLine(afterMove, 'low001')).toBe(taskLine(before, 'low001').replace('!4', '!1'));
    expect(afterMove.replace(taskLine(afterMove, 'low001'), '')).toBe(before.replace(taskLine(before, 'low001'), ''));

    await highest.getByRole('article', { name: /清理发布记录/ }).dragTo(page.getByRole('region', { name: /未定级/ }));
    await expect(page.getByRole('region', { name: /未定级 1/ }).getByText('清理发布记录')).toBeVisible();
    const afterClear = await taskFile();
    expect(taskLine(afterClear, 'low001')).toBe(taskLine(before, 'low001').replace(' !4', ''));
    expect(afterClear.replace(taskLine(afterClear, 'low001'), '')).toBe(before.replace(taskLine(before, 'low001'), ''));

    await restartServer();
    const freshPage = await browser.newPage({ baseURL: webUrl });
    await freshPage.goto('/');
    await freshPage.getByRole('button', { name: '象限' }).click();
    await expect(freshPage.getByRole('region', { name: /未定级 1/ }).getByText('清理发布记录')).toBeVisible();
    await freshPage.close();
  });

  test('quadrant keeps mixed-priority descendants inside their parent group without writing', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    const before = await taskFile();
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const group = page.locator(groupRoot('parent1'));
    await expect(group).toBeVisible();
    await expect(group.locator(taskNode('child01'))).toBeVisible();
    await expect(group.locator(taskNode('child02'))).toBeVisible();
    await expect(group.locator(taskNode('child01'))).toHaveAttribute('draggable', 'false');
    await expect(page.getByRole('region', { name: '!2 高' }).locator(groupRoot('parent1'))).toBeVisible();
    expect(await taskFile()).toBe(before);
  });

  test('quadrant group drag updates the whole subtree atomically without a scope menu', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const before = await taskFile();

    const parent = page.getByRole('article', { name: /发布桌面版/ });
    await parent.dragTo(page.getByRole('region', { name: '!1 最高' }));
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const highest = page.getByRole('region', { name: '!1 最高' });
    await expect(highest.locator(groupRoot('parent1'))).toBeVisible();
    await expect(highest.getByText('发布桌面版', { exact: true })).toBeVisible();
    await expect(highest.getByText('构建 macOS 安装包', { exact: true })).toBeVisible();
    await expect(highest.getByText('编写安装说明', { exact: true })).toBeVisible();

    const after = await taskFile();
    expect(taskLine(after, 'parent1')).toBe(taskLine(before, 'parent1').replace('!2', '!1'));
    expect(taskLine(after, 'child01')).toBe(taskLine(before, 'child01').replace('!3', '!1'));
    expect(taskLine(after, 'child02')).toBe(taskLine(before, 'child02').replace('!4', '!1'));
    for (const id of ['low001']) expect(taskLine(after, id)).toBe(taskLine(before, id));
  });


  test('narrow quadrant menu updates a whole group in one write', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    await page.setViewportSize({ width: 640, height: 950 });
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const before = await taskFile();
    const historyBefore = await (await page.request.get('/api/v1/history')).json();
    const parent = page.locator(taskNode('parent1'));
    await parent.getByRole('button', { name: '打开“发布桌面版”的任务菜单', exact: true }).click();
    await page.getByRole('menuitem', { name: '优先级…', exact: true }).click();
    await page.getByRole('menuitem', { name: /^!3 中/ }).click();
    await expect(page.getByRole('menu')).toHaveCount(0);
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('region', { name: '!3 中' }).locator(groupRoot('parent1'))).toBeVisible();
    const after = await taskFile();
    for (const id of ['parent1', 'child01', 'child02']) {
      expect(taskLine(after, id)).toBe(taskLine(before, id).replace('!1', '!3'));
    }
    const history = await page.request.get('/api/v1/history');
    expect(history.ok()).toBe(true);
    expect(await history.json()).toHaveLength(historyBefore.length + 1);
  });

  test('Escape closes a narrow group priority menu without writing', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    await page.setViewportSize({ width: 640, height: 950 });
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const before = await taskFile();
    await page.locator(taskNode('parent1')).getByRole('button', { name: '打开“发布桌面版”的任务菜单', exact: true }).click();
    const priority = page.getByRole('menuitem', { name: '优先级…', exact: true });
    await priority.focus();
    await priority.press('ArrowRight');
    await expect(page.getByRole('menu', { name: '设置优先级' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('menu')).toHaveCount(0);
    expect(await taskFile()).toBe(before);
  });

  test('parent group round trips into whitespace update the UI without changing hierarchy', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    const setup = await page.request.post('/api/v1/tasks/parent1/priority', { data: { priority: 2, scope: 'subtree' } });
    expect(setup.ok()).toBe(true);
    await page.setViewportSize({ width: 1440, height: 1200 });
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const before = await taskFile();
    expect(taskLine(before, 'child01')).toMatch(/^    - \[ \]/);
    const card = page.locator(taskNode('parent1'));

    for (const priority of [1, 2, 1, 2, 1, 2]) {
      const destination = page.getByRole('region', { name: priority === 1 ? '!1 最高' : '!2 高' });
      const bounds = await destination.boundingBox();
      if (!bounds) throw new Error('quadrant is not visible');
      const targetPosition = { x: bounds.width / 2, y: bounds.height - 18 };
      const targetIsCell = await page.evaluate(({ x, y }) => {
        return document.elementFromPoint(x, y)?.closest('section')?.getAttribute('aria-labelledby');
      }, { x: bounds.x + targetPosition.x, y: bounds.y + targetPosition.y });
      expect(targetIsCell).toBe(`quadrant-${priority}`);
      await card.dragTo(destination, { sourcePosition: { x: 180, y: 45 }, targetPosition });
      let expected = before;
      for (const id of ['parent1', 'child01', 'child02']) {
        expected = expected.replace(taskLine(before, id), taskLine(before, id).replace('!2', `!${priority}`));
      }
      await expect.poll(taskFile).toBe(expected);
      const group = destination.locator(groupRoot('parent1'));
      await expect(group).toBeVisible();
      await expect(group.locator(taskNode('child01'))).toBeVisible();
      await expect(group.locator(taskNode('child02'))).toBeVisible();
      await expect(page.getByRole('dialog')).toHaveCount(0);
    }
    expect(await taskFile()).toBe(before);
  });

  test('narrow desktop can drag a whole group between stacked quadrants', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    await page.setViewportSize({ width: 640, height: 1800 });
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const before = await taskFile();
    const destination = page.getByRole('region', { name: '!1 最高' });
    const parent = page.locator(taskNode('parent1'));
    expect(await parent.evaluate((node) => getComputedStyle(node).getPropertyValue('-webkit-user-drag'))).toBe('element');
    await parent.dragTo(destination, { sourcePosition: { x: 150, y: 45 } });
    const group = destination.locator(groupRoot('parent1'));
    await expect(group).toBeVisible();
    const after = await taskFile();
    for (const id of ['parent1', 'child01', 'child02']) {
      expect(taskLine(after, id)).toBe(taskLine(before, id).replace('!2', '!1'));
    }
    expect(await page.locator('body').evaluate((node) => node.scrollWidth <= window.innerWidth)).toBe(true);
  });

  test('a delayed refresh cannot overwrite a newer priority write', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    let releaseRead!: () => void;
    const readReleased = new Promise<void>((resolve) => { releaseRead = resolve; });
    let markReadCaptured!: () => void;
    const readCaptured = new Promise<void>((resolve) => { markReadCaptured = resolve; });
    let holdNextRead = true;
    await page.route('**/api/v1/snapshot', async (route) => {
      const hold = holdNextRead;
      holdNextRead = false;
      const response = await route.fetch();
      if (hold) {
        markReadCaptured();
        await readReleased;
      }
      await route.fulfill({ response });
    });
    try {
      await page.getByRole('button', { name: '重新载入 Snapshot' }).click();
      await readCaptured;
      const card = page.locator(taskNode('parent1'));
      await card.focus();
      await card.press('4');
      const destination = page.getByRole('region', { name: '!4 低' });
      await expect(destination.locator(groupRoot('parent1'))).toBeVisible();
      const oldResponse = page.waitForResponse((response) => response.url().endsWith('/api/v1/snapshot'));
      releaseRead();
      await oldResponse;
      await expect(destination.locator(groupRoot('parent1'))).toBeVisible();
    } finally {
      releaseRead();
      await page.unrouteAll({ behavior: 'wait' });
    }
  });

  test('strict done persists its note and Git evidence across reload', async ({ page }) => {
    const note = '真实服务验收通过';

    await page.goto('/');
    await page.getByRole('button', { name: /^完成“给登录接口加限流/ }).click();

    const dialog = page.getByRole('dialog', { name: '确认任务结局' });
    await expect(dialog.getByRole('checkbox')).toBeChecked();
    const evidence = await dialog.locator('code').textContent();
    expect(evidence).toMatch(/^[0-9a-f]+\.\.[0-9a-f]+$/);
    await dialog.getByRole('textbox', { name: '完成说明' }).fill(note);
    await dialog.getByRole('button', { name: '确认完成' }).click();

    await expect(page.getByText(note)).toBeVisible();

    await page.reload();
    await expect(page.getByText(note)).toBeVisible();
    const snapshot = await page.request.get('/api/v1/snapshot');
    expect(snapshot.ok()).toBeTruthy();
    const persisted = (await snapshot.json()).tasks.find((task: { body: string }) => task.body === '给登录接口加限流');
    expect(persisted.evidence).toBe(evidence);
  });

  test('restoring through the context menu preserves the outcome note and evidence', async ({ page }) => {
    await page.goto('/');
    const before = await page.request.get('/api/v1/snapshot');
    const completed = (await before.json()).tasks.find(
      (task: { body: string }) => task.body === '给登录接口加限流',
    );

    const card = page.getByRole('article', { name: /给登录接口加限流/ });
    await card.click({ button: 'right' });
    await page.getByRole('menuitem', { name: '恢复到 Todo' }).click();

    const reopened = await page.request.get('/api/v1/snapshot');
    const task = (await reopened.json()).tasks.find(
      (candidate: { body: string }) => candidate.body === '给登录接口加限流',
    );
    expect(task.completed).toBe(false);
    expect(task.column).toBe('todo');
    expect(task.note).toBe(completed.note);
    expect(task.evidence).toBe(completed.evidence);
  });

  test('writes reach the file through the API', async ({ page }) => {
    await page.goto('/');

    // A served project can aggregate several task files, so quick-add stays
    // disabled until one is named: writing a task into an unstated file is the
    // ambiguity this control exists to prevent.
    await page.getByRole('combobox', { name: '新增到来源' }).selectOption('TODO.md');

    const input = page.getByPlaceholder(/新增|添加/).first();
    await input.fill('从 Web 端新建的任务');
    await input.press('Enter');

    await expect(page.getByText('从 Web 端新建的任务')).toBeVisible();

    // A reload proves it was persisted, not just added to local state.
    await page.reload();
    await expect(page.getByText('从 Web 端新建的任务')).toBeVisible();
  });

  test('scanning runs against the real repository', async ({ page }) => {
    await page.goto('/');
    await selectSingleSource(page);
    await page.getByRole('button', { name: /扫描与历史/ }).click();
    await page.getByRole('button', { name: '扫描', exact: true }).click();

    await expect(page.getByText(/扫描 \d+ 个文件/)).toBeVisible();
  });

  test('version history reflects writes made through the API', async ({ page }) => {
    // Self-sufficient: specs run in parallel against one server, so this cannot
    // depend on another test having written first.
    await page.goto('/');
    await page.getByRole('combobox', { name: '新增到来源' }).selectOption('TODO.md');
    const input = page.getByPlaceholder(/新增|添加/).first();
    await input.fill(`历史用例 ${Date.now()}`);
    await input.press('Enter');

    await selectSingleSource(page);
    await page.getByRole('button', { name: /扫描与历史/ }).click();
    await page.getByRole('button', { name: '刷新历史' }).click();

    await expect(page.locator('.review-list--history li').first()).toBeVisible();
    // Every restore point records the action that caused it, not a bare "update".
    await expect(page.getByText('board-add').first()).toBeVisible();
  });
});
