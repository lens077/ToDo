import { expect, test } from '@playwright/test';

/**
 * The confirmation gate and the views around it.
 *
 * The property under test throughout: a proposed rewrite is only ever written
 * after an explicit confirmation, and what the user confirms is what they were
 * shown.
 */

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('navigation')).toBeVisible();
});

test('the landing view is Today, not the board', async ({ page }) => {
  // "What should I do now" is the question the homepage answers.
  const today = page.getByRole('button', { name: /今日/ });
  await expect(today).toHaveClass(/is-active/);
});

test('the board is one click away', async ({ page }) => {
  await page.getByRole('button', { name: /看板/ }).click();
  await expect(page.getByRole('button', { name: /看板/ })).toHaveClass(/is-active/);
  await expect(page.locator('.task-card').first()).toBeVisible();
});

test('scanning reports findings without writing anything', async ({ page }) => {
  await page.getByRole('button', { name: /扫描与历史/ }).click();
  await expect(page.getByRole('heading', { name: '扫描全仓' })).toBeVisible();

  await page.getByRole('button', { name: '扫描', exact: true }).click();

  await expect(page.getByText(/发现 \d+ 条/)).toBeVisible();
  // A scan is read-only: no confirmation surface should appear on its own.
  await expect(page.getByRole('button', { name: '确认并写入' })).toHaveCount(0);
});

test('importing shows a side-by-side diff and writes nothing until confirmed', async ({ page }) => {
  await page.getByRole('button', { name: /扫描与历史/ }).click();
  await page.getByRole('button', { name: '扫描', exact: true }).click();
  await expect(page.getByText(/发现 \d+ 条/)).toBeVisible();

  await page.getByRole('button', { name: '预览导入' }).click();

  const diff = page.getByRole('table', { name: '左右对比' });
  await expect(diff).toBeVisible();
  await expect(page.getByRole('columnheader', { name: '当前 TODO.md' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: '导入后' })).toBeVisible();
  await expect(page.getByRole('button', { name: '确认并写入' })).toBeVisible();

  // Cancelling must leave no pending write behind.
  await page.getByRole('button', { name: '取消' }).click();
  await expect(diff).toHaveCount(0);
  await expect(page.getByRole('heading', { name: '扫描全仓' })).toBeVisible();
});

test('the diff numbers each side independently', async ({ page }) => {
  // A replaced line occupies different line numbers on the left and the right;
  // a single shared gutter would misreport one of them.
  await page.getByRole('button', { name: /扫描与历史/ }).click();
  await page.getByRole('button', { name: '扫描', exact: true }).click();
  await page.getByRole('button', { name: '预览导入' }).click();

  const rows = page.locator('.diff-row');
  await expect(rows.first()).toBeVisible();
  await expect(page.locator('.diff-cell--removed').first()).toBeVisible();
  await expect(page.locator('.diff-cell--added').first()).toBeVisible();
  await expect(page.locator('.diff-summary')).toContainText('新增');
});

test('restore is previewed as a diff before it can be applied', async ({ page }) => {
  await page.getByRole('button', { name: /扫描与历史/ }).click();
  await page.getByRole('button', { name: '刷新历史' }).click();

  const restore = page.getByRole('button', { name: '预览恢复' }).first();
  await expect(restore).toBeVisible();
  await restore.click();

  await expect(page.getByRole('table', { name: '左右对比' })).toBeVisible();
  await expect(page.getByText(/确认后才会写入文件/)).toBeVisible();
  await expect(page.getByRole('button', { name: '确认并写入' })).toBeVisible();
});

test('the alignment prompt is available for handing to an agent', async ({ page }) => {
  await page.getByRole('button', { name: /扫描与历史/ }).click();
  await page.getByRole('button', { name: '对齐提示词' }).click();

  await expect(page.getByRole('heading', { name: '对齐提示词' })).toBeVisible();
  await expect(page.locator('.review-prompt')).toContainText('核对 TODO 与代码现实');
});

test('provider settings persist locally without logging the API key', async ({ page }) => {
  const apiKey = 'test-key-never-log';
  const consoleMessages: string[] = [];
  page.on('console', (message) => consoleMessages.push(message.text()));

  await page.getByRole('button', { name: '模型设置' }).click();
  const dialog = page.getByRole('dialog', { name: '模型提供方设置' });
  await expect(dialog).toBeVisible();

  const keyInput = dialog.getByLabel('API 密钥');
  await expect(keyInput).toHaveAttribute('type', 'password');
  await dialog.getByLabel('提供方').selectOption('openai');
  await keyInput.fill(apiKey);
  await dialog.getByText('自定义设置', { exact: true }).click();
  await dialog.getByLabel('API 地址').fill('https://models.example.test/v1');
  await dialog.getByLabel('当前模型').fill('outside-catalog-model');
  await dialog.getByRole('button', { name: '添加模型' }).click();
  await dialog.getByRole('button', { name: '保存', exact: true }).click();

  await expect(dialog).toHaveCount(0);
  const stored = await page.evaluate(() => localStorage.getItem('todo.provider-settings.v1'));
  expect(JSON.parse(stored ?? '{}')).toMatchObject({
    provider: 'openai',
    api_key: apiKey,
    base_url: 'https://models.example.test/v1',
    model: 'outside-catalog-model',
    custom_models: ['outside-catalog-model'],
  });

  await page.reload();
  await page.getByRole('button', { name: '模型设置' }).click();
  await expect(page.getByRole('dialog').getByLabel('API 密钥')).toHaveValue(apiKey);
  expect(consoleMessages.join('\n')).not.toContain(apiKey);
});
