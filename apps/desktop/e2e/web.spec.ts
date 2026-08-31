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

test.describe('web transport', () => {
  test.skip(!webUrl, 'set E2E_WEB_URL to a running `todo serve` to enable');

  test.use({ baseURL: webUrl });

  test('loads real tasks from the HTTP backend', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('navigation')).toBeVisible();
    // The fixture project has a task the mock data does not, so seeing it proves
    // the data came from the server rather than the built-in fixtures.
    await expect(page.getByText('给登录接口加限流')).toBeVisible();
  });

  test('writes reach the file through the API', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: /看板/ }).click();

    const input = page.getByPlaceholder(/新增|添加/).first();
    await input.fill('从 Web 端新建的任务');
    await input.press('Enter');

    await expect(page.getByText('从 Web 端新建的任务')).toBeVisible();

    // A reload proves it was persisted, not just added to local state.
    await page.reload();
    await page.getByRole('button', { name: /看板/ }).click();
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
    await page.getByRole('button', { name: /看板/ }).click();
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
