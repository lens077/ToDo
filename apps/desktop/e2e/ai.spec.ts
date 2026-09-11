import { expect, test } from '@playwright/test';

/**
 * The AI view against the mock feed (`desktop.ts`, `?ai=` modes).
 *
 * The fixture holds seven sessions: four under the mock project root
 * (`waiting`, `failed`, `running`, and an `unread` one in a subdirectory), one
 * `unread` in another workspace, one `seen`, one `handled`. Attention is
 * `waiting + failed + unread`, so the project badge reads 3 and the
 * all-workspaces badge reads 4.
 */
const nav = (page: import('@playwright/test').Page) => page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: 'AI' });

test.describe('AI view', () => {
  test('sidebar badge counts attention for the current scope and follows the toggle', async ({ page }) => {
    await page.goto('/');
    await expect(nav(page)).toContainText('3');
    await nav(page).click();
    await expect(page.getByRole('heading', { name: 'AI', level: 1 })).toBeVisible();
    const scope = page.getByRole('group', { name: '会话范围' });
    await scope.getByRole('button', { name: '所有工作区' }).click();
    await expect(nav(page)).toContainText('4');
    await expect(page.getByTestId('ai-card').filter({ hasText: '把博客构建脚本迁到 pnpm' })).toBeVisible();
    await scope.getByRole('button', { name: '当前项目' }).click();
    await expect(nav(page)).toContainText('3');
    await expect(page.getByTestId('ai-card').filter({ hasText: '把博客构建脚本迁到 pnpm' })).toHaveCount(0);
  });

  test('sections follow the contract order and handled is hidden by default', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    await expect(page.locator('.ai-section h2')).toHaveText(['等你回复', '失败', '完成未读', '运行中', '已读']);
    await expect(page.getByTestId('ai-card').filter({ hasText: '确定 TODO.md v1 的保真写回边界' })).toHaveCount(0);
    await page.getByRole('button', { name: /^已处理/ }).click();
    await expect(page.locator('.ai-section h2').last()).toHaveText('已处理');
    await expect(page.getByTestId('ai-card').filter({ hasText: '确定 TODO.md v1 的保真写回边界' })).toBeVisible();
  });

  test('cards show state, workspace, question, files, and an untitled fallback', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    const failed = page.getByTestId('ai-card').filter({ hasText: '修复 watcher 的读取反馈循环' });
    await expect(failed.locator('.ai-badge')).toHaveText('失败');
    await expect(failed.locator('.ai-card__outcome')).toHaveText('出错');
    await expect(failed.locator('.ai-card__workspace')).toHaveText('field-notes');
    await expect(failed.locator('.ai-card__question')).toContainText('「把 Linux 上的 Access(Open) 事件过滤掉」');

    const unread = page.getByTestId('ai-card').filter({ hasText: '补齐 evidence diff 摘要' });
    await expect(unread.locator('.ai-card__files li')).toHaveCount(4);
    await expect(unread.locator('.ai-card__files-more')).toHaveText('+4');
    await expect(unread.locator('.ai-card__reply p')).toHaveClass(/is-clamped/);
    await unread.getByRole('button', { name: '展开' }).click();
    await expect(unread.locator('.ai-card__reply p')).not.toHaveClass(/is-clamped/);

    await expect(page.getByTestId('ai-card').filter({ hasText: '未命名会话' })).toBeVisible();
  });

  test('a state pill toggles its section', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    const running = page.getByRole('button', { name: /^运行中/ });
    await expect(running).toHaveAttribute('aria-pressed', 'true');
    await running.click();
    await expect(page.locator('.ai-section h2')).toHaveText(['等你回复', '失败', '完成未读', '已读']);
    await running.click();
    await expect(page.locator('.ai-section h2')).toHaveText(['等你回复', '失败', '完成未读', '运行中', '已读']);
  });

  test('marking a session read moves it out of 完成未读 and off the badge', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    const card = page.getByTestId('ai-card').filter({ hasText: '补齐 evidence diff 摘要' });
    await expect(page.getByRole('region', { name: '完成未读' })).toContainText('补齐 evidence diff 摘要');
    await card.getByRole('button', { name: '标记已读' }).click();
    await expect(page.getByRole('region', { name: '已读' })).toContainText('补齐 evidence diff 摘要');
    await expect(page.getByRole('region', { name: '完成未读' })).toHaveCount(0);
    await expect(nav(page)).toContainText('2');
    await expect(card.getByRole('button', { name: '标记已读' })).toHaveCount(0);
  });

  test('removing a session hides it and lowers the badge', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    const card = page.getByTestId('ai-card').filter({ hasText: '为 CLI 增加 release 构建说明' });
    await card.getByRole('button', { name: /^移除会话/ }).click();
    await expect(card).toHaveCount(0);
    await expect(page.getByRole('region', { name: '等你回复' })).toHaveCount(0);
    await expect(nav(page)).toContainText('2');
  });

  test('转为任务 creates a linked Project Task with a session note', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    const card = page.getByTestId('ai-card').filter({ hasText: '为 CLI 增加 release 构建说明' });
    await card.getByRole('button', { name: '转为任务' }).click();
    await expect(card.locator('.ai-card__linked')).toContainText('已转为任务');
    await expect(card.getByRole('button', { name: '转为任务' })).toHaveCount(0);
    await page.getByRole('button', { name: '目标' }).click();
    const task = page.getByRole('article').filter({ hasText: '为 CLI 增加 release 构建说明' });
    await expect(task).toBeVisible();
    await expect(task).toContainText('来自 dsh 会话 s_wait1：http://127.0.0.1:3080/?session=s_wait1');
  });

  test('打开会话 is disabled without a link and opens the URL otherwise', async ({ page, context }) => {
    await page.goto('/');
    await nav(page).click();
    const card = page.getByTestId('ai-card').filter({ hasText: '为 CLI 增加 release 构建说明' });
    const popup = context.waitForEvent('page');
    await card.getByRole('button', { name: '打开会话' }).click();
    const opened = await popup;
    expect(opened.url()).toBe('http://127.0.0.1:3080/?session=s_wait1');
    await opened.close();
  });

  test('empty states: no host yet, connected but nothing here, and everything filtered out', async ({ page }) => {
    await page.goto('/?ai=none');
    await nav(page).click();
    await expect(nav(page).locator('.primary-nav__count')).toHaveCount(0);
    const notConnected = page.getByTestId('ai-empty-not-connected');
    await expect(notConnected).toBeVisible();
    await expect(notConnected).toContainText('todo hooks instructions --agent deepseek-harness');
    await expect(notConnected).toContainText('docs/ai-feed-spec.md');

    await page.goto('/?ai=empty');
    await nav(page).click();
    await expect(page.getByTestId('ai-empty-none')).toBeVisible();
    await expect(page.getByTestId('ai-empty-none')).not.toContainText('todo hooks instructions');

    await page.goto('/');
    await nav(page).click();
    for (const name of ['运行中', '等你回复', '完成未读', '失败', '已读']) {
      await page.getByRole('button', { name: new RegExp(`^${name}`) }).click();
    }
    await expect(page.getByTestId('ai-empty-filtered')).toBeVisible();
  });

  test('without a project the scope toggle is gone and 转为任务 is disabled', async ({ page }) => {
    await page.goto('/?mock=no-project');
    await nav(page).click();
    await expect(page.getByRole('group', { name: '会话范围' })).toHaveCount(0);
    await expect(nav(page)).toContainText('4');
    const card = page.getByTestId('ai-card').filter({ hasText: '为 CLI 增加 release 构建说明' });
    await expect(card.getByRole('button', { name: '转为任务' })).toBeDisabled();
  });
});
