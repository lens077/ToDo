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

async function taskFile(): Promise<string> {
  if (!fixturePath) throw new Error('E2E_TODO_PATH is required for file assertions');
  const { readFile } = await import('node:fs/promises');
  return readFile(fixturePath, 'utf8');
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

    const freshPage = await browser.newPage({ baseURL: webUrl });
    await freshPage.goto('/');
    await freshPage.getByRole('button', { name: '象限' }).click();
    await expect(freshPage.getByRole('region', { name: /未定级 1/ }).getByText('清理发布记录')).toBeVisible();
    await freshPage.close();
  });

  test('parent priority asks for scope and subtree update is atomic', async ({ page }) => {
    test.skip(!fixturePath, 'E2E_TODO_PATH is required');
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
    const before = await taskFile();

    const parent = page.getByRole('article', { name: /发布桌面版/ });
    await parent.dragTo(page.getByRole('region', { name: '!1 最高' }));
    const dialog = page.getByRole('dialog', { name: '这个任务包含子任务' });
    await expect(dialog.getByRole('button', { name: '仅修改这一项' })).toBeVisible();
    await expect(dialog.getByRole('button', { name: '同时修改 2 个子任务' })).toBeVisible();
    await dialog.getByRole('button', { name: '同时修改 2 个子任务' }).click();
    await expect(page.getByRole('region', { name: '!1 最高' }).getByText('发布桌面版')).toBeVisible();

    const after = await taskFile();
    expect(taskLine(after, 'parent1')).toBe(taskLine(before, 'parent1').replace('!2', '!1'));
    expect(taskLine(after, 'child01')).toBe(taskLine(before, 'child01').replace('!3', '!1'));
    expect(taskLine(after, 'child02')).toBe(taskLine(before, 'child02').replace('!4', '!1'));
    for (const id of ['low001']) expect(taskLine(after, id)).toBe(taskLine(before, id));
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
    await page.getByRole('button', { name: /扫描与历史/ }).click();
    await page.getByRole('button', { name: '扫描', exact: true }).click();

    await expect(page.getByText(/扫描 \d+ 个文件/)).toBeVisible();
  });

  test('version history reflects writes made through the API', async ({ page }) => {
    // Self-sufficient: specs run in parallel against one server, so this cannot
    // depend on another test having written first.
    await page.goto('/');
    const input = page.getByPlaceholder(/新增|添加/).first();
    await input.fill(`历史用例 ${Date.now()}`);
    await input.press('Enter');

    await page.getByRole('button', { name: /扫描与历史/ }).click();
    await page.getByRole('button', { name: '刷新历史' }).click();

    await expect(page.locator('.review-list--history li').first()).toBeVisible();
    // Every restore point records the action that caused it, not a bare "update".
    await expect(page.getByText('board-add').first()).toBeVisible();
  });
});
