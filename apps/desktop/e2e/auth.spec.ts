import { expect, test, type Page } from '@playwright/test';

const signedIn = {
  signed_in: true,
  pro: true,
  sub: 'u-casdoor-7f3a',
  device_name: 'MacBook · macOS',
  bound_at: 1700000000,
  token_expires_at: 1700043200,
  pending: false,
  proxy_base: 'https://virgo-img.apikv.com/img?url=',
  proxy_enabled: true,
  error: null,
};

async function openSettings(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: '设置', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '设置' });
  await expect(dialog).toBeVisible();
  return dialog;
}

// 图片代理从「手填地址+令牌」改为「登录 Pro 账号」：设置里不再有可粘贴的
// 令牌输入框，登录态与设备绑定信息由 Rust 提供。
test('settings shows the Pro login state instead of manual proxy fields', async ({ page }) => {
  const dialog = await openSettings(page);

  await expect(dialog.getByRole('heading', { name: '图片代理 Pro' })).toBeVisible();
  // 旧的手填字段必须消失：令牌不再经过 localStorage。
  await expect(dialog.getByLabel('代理地址')).toHaveCount(0);
  await expect(dialog.getByLabel('访问令牌')).toHaveCount(0);

  const signedOut = dialog.locator('[data-auth-state="signed-out"]');
  await expect(signedOut).toBeVisible();
  await expect(signedOut).toContainText('不经过任何第三方');

  await dialog.getByRole('button', { name: '登录 Pro 账号' }).click();
  const signedInPanel = dialog.locator('[data-auth-state="signed-in"]');
  await expect(signedInPanel).toBeVisible();
  await expect(signedInPanel).toContainText('Mockbook · macOS');
  await expect(signedInPanel).toContainText('Pro 有效');
  await expect(signedInPanel).toContainText('令牌剩余约');

  // 令牌只在 Rust 内存里：设置存储里不该出现任何令牌字段。
  const stored = await page.evaluate(() => window.localStorage.getItem('todo.app-settings.v1') ?? '{}');
  expect(stored).not.toContain('imageProxyToken');
  expect(stored).not.toContain('imageProxyBase');
});

// 解绑要二次确认（换机有 24 小时冷却），确认后回到未登录态。
test('unbinding is a two-step action and signs the device out', async ({ page }) => {
  await page.addInitScript((status) => {
    (window as unknown as { __todoMockAuth?: unknown }).__todoMockAuth = status;
  }, signedIn);
  const dialog = await openSettings(page);

  const signedInPanel = dialog.locator('[data-auth-state="signed-in"]');
  await expect(signedInPanel).toBeVisible();
  await expect(signedInPanel).toContainText('MacBook · macOS');

  // 先点「解绑这台设备」只进入确认态，不真的解绑。
  await signedInPanel.getByRole('button', { name: '解绑这台设备' }).click();
  await expect(signedInPanel).toContainText('24 小时内只能解绑一次');
  await expect(dialog.locator('[data-auth-state="signed-in"]')).toBeVisible();

  await signedInPanel.getByRole('button', { name: '取消' }).click();
  await expect(signedInPanel).not.toContainText('24 小时内只能解绑一次');

  await signedInPanel.getByRole('button', { name: '解绑这台设备' }).click();
  await signedInPanel.getByRole('button', { name: '确认解绑' }).click();
  await expect(dialog.locator('[data-auth-state="signed-out"]')).toBeVisible();
});
