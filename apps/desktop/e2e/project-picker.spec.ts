import { expect, test } from '@playwright/test';

for (const entry of ['.project-switcher', '.command-project']) {
  test(`${entry} opens an honest browser project chooser and restores focus`, async ({ page }) => {
    await page.goto('/');
    const before = await page.locator('.project-switcher strong').textContent();
    const tasks = await page.locator('[data-task-id]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-task-id')));
    await page.locator(entry).click();
    const dialog = page.getByRole('dialog', { name: '打开项目' });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText('当前为演示模式', { exact: true })).toBeVisible();
    await expect(dialog.getByRole('link', { name: '打开本机项目服务' })).toHaveAttribute('href', 'http://127.0.0.1:3017/');
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(page.locator(entry)).toBeFocused();
    await expect(page.locator('.project-switcher strong')).toHaveText(before!);
    expect(await page.locator('[data-task-id]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-task-id')))).toEqual(tasks);
  });
}

test('invalid project service address is rejected without changing the project', async ({ page }) => {
  await page.goto('/');
  await page.locator('.project-switcher').click();
  const dialog = page.getByRole('dialog', { name: '打开项目' });
  await dialog.getByLabel('本机项目服务地址').fill('https://example.com/');
  await expect(dialog.getByRole('alert')).toContainText('本机');
  await expect(dialog.getByRole('link', { name: '打开本机项目服务' })).toHaveCount(0);
  await dialog.getByRole('button', { name: '取消' }).click();
  await expect(page.locator('.project-switcher strong')).toHaveText('field-notes');
});

test('HTTP project selection never replaces a server project with demo data', async ({ page }) => {
  await page.route('**/api/v1/health', (route) => route.fulfill({ json: { ok: true, read_only: false, project: '/real-project/TODO.md' } }));
  await page.route('**/api/v1/snapshot', (route) => route.fulfill({ json: { project_path: '/real-project', revision: 'test', columns: [], tasks: [], today: {} } }));
  await page.goto('/');
  await expect(page.locator('.project-switcher strong')).toHaveText('real-project');
  await page.locator('.command-project').click();
  const dialog = page.getByRole('dialog', { name: '打开项目' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('当前连接到项目服务', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '取消' }).click();
  await expect(page.locator('.project-switcher strong')).toHaveText('real-project');
  await expect(page.locator('[data-task-id]')).toHaveCount(0);
});

test('read-only HTTP project entry remains usable for connection guidance', async ({ page }) => {
  await page.route('**/api/v1/health', (route) => route.fulfill({ json: { ok: true, read_only: true, project: '/real-project/TODO.md' } }));
  await page.goto('/');
  await page.locator('.project-switcher').click();
  const dialog = page.getByRole('dialog', { name: '打开项目' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('当前连接到项目服务', { exact: true })).toBeVisible();
  await expect(dialog.getByText('当前为演示模式', { exact: true })).toHaveCount(0);
});
