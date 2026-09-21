import { expect, test, type Page } from '@playwright/test';

const task = {
  id: 'mobile1',
  body: '手机上核对语义对账状态',
  completed: false,
  column: 'doing',
  due: '2026-09-01',
  priority: 1,
  tags: ['mobile'],
  progress: 60,
  failed_on: '2026-08-31',
  note: '保留状态物证',
};

const readOnlySnapshot = {
  project_path: '/fixtures/mobile/TODO.md',
  revision: '42',
  columns: [
    { id: 'todo', name: '待办', done: false },
    { id: 'doing', name: '进行中', done: false },
    { id: 'done', name: '完成', done: true },
  ],
  tasks: [task],
  today: {
    overdue: [],
    today: [task],
    upcoming: [],
    next: [],
  },
};

async function useReadOnlyBackend(page: Page) {
  await page.route('**/api/v1/health', (route) =>
    route.fulfill({ json: { ok: true, read_only: true, project: 'TODO.md' } }),
  );
  await page.route('**/api/v1/snapshot', (route) => route.fulfill({ json: readOnlySnapshot }));
  await page.route('**/api/v1/history', (route) =>
    route.fulfill({
      json: [
        {
          version: 1,
          at: '2026-09-01T08:00:00Z',
          file: 'TODO.md',
          op: 'board-add',
          bytes: 128,
          blob: 'v000001.snap',
        },
      ],
    }),
  );
}

test('mobile navigation stays reachable at the bottom', async ({ page }) => {
  await page.goto('/');

  const sidebar = page.locator('.sidebar');
  const workspace = page.locator('.workspace');
  await expect(page.getByRole('navigation', { name: '主要视图' })).toBeVisible();
  expect(await page.evaluate(() => navigator.maxTouchPoints)).toBeGreaterThan(0);

  const sidebarBox = await sidebar.boundingBox();
  const workspaceBox = await workspace.boundingBox();
  expect(sidebarBox).not.toBeNull();
  expect(workspaceBox).not.toBeNull();
  expect(sidebarBox!.y).toBeGreaterThanOrEqual(workspaceBox!.y + workspaceBox!.height - 1);

  const navButtons = page.getByRole('navigation', { name: '主要视图' }).getByRole('button');
  await expect(navButtons).toHaveCount(5);
  for (const button of await navButtons.all()) {
    const box = await button.boundingBox();
    expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
  }

  await page.getByRole('button', { name: '目标', exact: true }).click();
  await expect(page.getByRole('heading', { name: '目标' })).toBeVisible();
});

test('goal regions stack without horizontal scrolling', async ({ page }) => {
  await page.goto('/');

  const regions = page.locator('.goal-region');
  await expect(regions).toHaveCount(3);
  const layout = await regions.evaluateAll((items) =>
    items.map((item) => {
      const box = item.getBoundingClientRect();
      return { left: Math.round(box.left), width: Math.round(box.width) };
    }),
  );
  expect(new Set(layout.map((region) => region.left)).size).toBe(1);
  const viewportWidth = page.viewportSize()?.width ?? 0;
  expect(layout.every((region) => region.width <= viewportWidth)).toBe(true);

  const overflow = await page.locator('.workspace__content').evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
  }));
  expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth + 1);
});

test('touch menu can restore a completed task to the top of Todo', async ({ page }) => {
  await page.goto('/');
  const focusButtonBounds = await page.locator('.task-card--focus .move-menu').boundingBox();
  expect(focusButtonBounds?.width).toBeGreaterThan(44);
  expect(focusButtonBounds?.height).toBeGreaterThanOrEqual(44);

  const completed = page.getByRole('article', { name: /确定 TODO\.md v1 的保真写回边界/ });
  const menuButton = completed.getByRole('button', { name: /打开“确定 TODO\.md v1.*任务菜单/ });
  const buttonBounds = await menuButton.boundingBox();
  expect(buttonBounds?.width).toBeGreaterThanOrEqual(44);
  expect(buttonBounds?.height).toBeGreaterThanOrEqual(44);
  await menuButton.click();
  await page.getByRole('menuitem', { name: '恢复到 待梳理' }).click();

  const first = page.locator('[aria-label="待梳理"]').getByRole('article').first();
  await expect(first).toHaveAccessibleName(/未完成 Project Task：确定 TODO\.md v1/);

  await first.getByRole('button', { name: /打开“确定 TODO\.md v1.*任务菜单/ }).click();
  await page.getByRole('menuitem', { name: '移动到…' }).click();
  const submenu = page.getByRole('menu', { name: '移动到列' });
  const bounds = await submenu.boundingBox();
  const viewportWidth = page.viewportSize()?.width ?? 0;
  expect(bounds).not.toBeNull();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.width).toBeGreaterThanOrEqual(200);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(viewportWidth);
  await submenu.getByRole('menuitem', { name: '等待验证' }).click();
  await expect(page.locator('[aria-label="等待验证"]').getByRole('article').first())
    .toHaveAccessibleName(/未完成 Project Task：确定 TODO\.md v1/);
});

test('read-only mode disables task, capture, import, and restore writes', async ({ page }) => {
  const captureRequests: string[] = [];
  page.on('request', (request) => { if (new URL(request.url()).pathname.includes('/api/v1/captures')) captureRequests.push(request.url()); });
  await useReadOnlyBackend(page);
  await page.goto('/');

  await expect(page.getByTestId('read-only-notice')).toContainText('只读模式');
  await expect(page.getByLabel('快速新增 Project Task')).toBeDisabled();
  await expect(page.getByRole('button', { name: '只读', exact: true })).toBeDisabled();

  const card = page.getByRole('article', { name: /手机上核对语义对账状态/ });
  await expect(card.getByLabel('完成“手机上核对语义对账状态”')).toBeDisabled();
  expect(await card.evaluate((element) => element.draggable)).toBe(false);
  await expect(card.locator('.move-menu')).toHaveCount(0);
  await expect(card.getByLabel('优先级 1')).toBeVisible();
  await expect(card.locator('time[datetime="2026-09-01"]')).toBeVisible();
  await expect(card.getByLabel('已完成 60%')).toBeVisible();
  await expect(card.getByLabel('上次失败于 2026-08-31')).toBeVisible();

  await page.getByRole('button', { name: '随记', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'HTTP 项目服务不开放个人随记' })).toBeVisible();
  await expect(page.getByLabel('记下一件还没想清楚的事')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /删除 Capture/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /分拣为任务/ })).toHaveCount(0);
  expect(captureRequests).toEqual([]);

  await page.getByRole('button', { name: '扫描与历史', exact: true }).click();
  const previewImport = page.getByRole('button', { name: '预览导入' });
  await expect(previewImport).toBeDisabled();
  await expect(previewImport).toHaveAttribute('title', '只读模式下不能导入');
  await page.getByRole('button', { name: '刷新历史' }).click();
  const previewRestore = page.getByRole('button', { name: '预览恢复' });
  await expect(previewRestore).toBeDisabled();
  await expect(previewRestore).toHaveAttribute('title', '只读模式下不能恢复');
});

test('PWA manifest and declared icons load', async ({ request }) => {
  const response = await request.get('/manifest.webmanifest');
  expect(response.ok()).toBe(true);
  const manifest = (await response.json()) as {
    name: string;
    display: string;
    lang: string;
    icons: Array<{ src: string; purpose: string }>;
  };
  expect(manifest).toMatchObject({ name: 'Sumery Virgo', display: 'standalone', lang: 'zh-CN' });
  expect(manifest.icons.some((icon) => icon.purpose === 'maskable')).toBe(true);

  for (const icon of manifest.icons) {
    const iconResponse = await request.get(icon.src);
    expect(iconResponse.ok()).toBe(true);
    expect(iconResponse.headers()['content-type']).toContain('image/png');
  }
});
