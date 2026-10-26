import { expect, test } from '@playwright/test';

/**
 * The AI view against the mock feed (`desktop.ts`, `?ai=` modes).
 *
 * The fixture holds eight sessions: five under the mock project root
 * (`waiting`, `failed`, `running`, an `unread` one in a subdirectory, and a
 * `seen` one with 32 absolute paths and a 300-character token), one `unread`
 * in another workspace, one `seen`, one `handled`. Attention is
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
    // 不依赖目标 URL 真的能加载：无服务时 Chromium 会把地址改成
    // chrome-error://chromewebdata/。这里验收应用交给窗口的 URL。
    await expect
      .poll(() => page.evaluate(() => window.__todoMockOpenSessions ?? []))
      .toEqual([{ url: 'http://127.0.0.1:3080/?session=s_wait1', mode: 'reuse' }]);
    await opened.close();
    await expect(page.getByTestId('ai-card').filter({ hasText: '把 1,000 项 Snapshot' }).getByRole('button', { name: '打开会话' })).toBeDisabled();
  });

  test('the 打开会话 setting persists and decides how a session opens', async ({ page, context }) => {
    await page.goto('/');
    await nav(page).click();
    const card = page.getByTestId('ai-card').filter({ hasText: '为 CLI 增加 release 构建说明' });
    const recorded = () => page.evaluate(() => window.__todoMockOpenSessions ?? []);

    // Default: reuse one window.
    let popup = context.waitForEvent('page');
    await card.getByRole('button', { name: '打开会话' }).click();
    await (await popup).close();
    expect(await recorded()).toEqual([{ url: 'http://127.0.0.1:3080/?session=s_wait1', mode: 'reuse' }]);

    await page.getByRole('button', { name: '设置', exact: true }).click();
    const settings = page.getByRole('dialog', { name: '设置' });
    const group = settings.getByRole('radiogroup', { name: '打开会话' });
    await expect(group.getByRole('radio', { name: '复用同一窗口（默认）' })).toBeChecked();
    await expect(settings).toContainText('在 ToDo 自带的窗口里打开 DSH，再次打开时切换会话，不新开标签页');
    await group.getByRole('radio', { name: '每次新开标签页' }).check();
    await expect(settings).toContainText('用系统默认浏览器打开');
    await page.keyboard.press('Escape');
    await expect(settings).toBeHidden();

    popup = context.waitForEvent('page');
    await card.getByRole('button', { name: '打开会话' }).click();
    await (await popup).close();
    expect((await recorded()).at(-1)).toEqual({ url: 'http://127.0.0.1:3080/?session=s_wait1', mode: 'new-tab' });

    await page.reload();
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem('todo.app-settings.v1') ?? '{}').openSession)).toBe('new-tab');
    await page.getByRole('button', { name: '设置', exact: true }).click();
    await expect(page.getByRole('radiogroup', { name: '打开会话' }).getByRole('radio', { name: '每次新开标签页' })).toBeChecked();
  });

  test('wide window: four cards per row, and no card leaves its column', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/');
    await nav(page).click();
    const wide = page.getByTestId('ai-card').filter({ hasText: '整理 gateway 组件的 README 与部署清单' });
    await expect(wide).toBeVisible();
    // Absolute paths under the workspace are shown relative to it, the rest
    // of the 32 as a count.
    await expect(wide.locator('.ai-card__files li code').first()).toHaveText('components/gateway/README.md');
    await expect(wide.locator('.ai-card__files li').first()).toHaveAttribute('title', '/Users/demo/field-notes/kubernetes/components/gateway/README.md');
    await expect(wide.locator('.ai-card__files li:not(.ai-card__files-more)')).toHaveCount(3);
    await expect(wide.locator('.ai-card__files-more')).toHaveText('+29');
    // Long question and reply clamp and offer the same 展开.
    await expect(wide.locator('.ai-card__question p')).toHaveClass(/is-clamped/);
    await expect(wide.locator('.ai-card__question').getByRole('button', { name: '展开' })).toBeVisible();
    await expect(wide.locator('.ai-card__reply').getByRole('button', { name: '展开' })).toBeVisible();

    const geometry = await page.evaluate(() => {
      const sections = Array.from(document.querySelectorAll<HTMLElement>('.ai-section'));
      return sections.map((section) => {
        const bounds = section.getBoundingClientRect();
        const grid = section.querySelector<HTMLElement>('.ai-section__cards')!;
        // The resolved value of an auto-fill grid lists one size per track.
        const columns = getComputedStyle(grid).gridTemplateColumns.trim().split(/\s+/).length;
        return {
          right: bounds.right,
          columns,
          cards: Array.from(section.querySelectorAll<HTMLElement>('.ai-card')).map((card) => {
            const rect = card.getBoundingClientRect();
            return { right: rect.right, scrollWidth: card.scrollWidth, clientWidth: card.clientWidth };
          }),
        };
      });
    });
    expect(geometry.length).toBeGreaterThan(0);
    for (const section of geometry) {
      expect(section.columns).toBe(4);
      for (const card of section.cards) {
        expect(card.right).toBeLessThanOrEqual(section.right + 0.5);
        expect(card.scrollWidth).toBeLessThanOrEqual(card.clientWidth + 1);
      }
    }

    // Expanding the unbroken token must wrap it, not widen the card.
    await wide.locator('.ai-card__question').getByRole('button', { name: '展开' }).click();
    const after = await wide.evaluate((card) => {
      const section = card.closest('.ai-section')!;
      return { card: card.getBoundingClientRect().right, section: section.getBoundingClientRect().right, scrollWidth: card.scrollWidth, clientWidth: card.clientWidth };
    });
    expect(after.card).toBeLessThanOrEqual(after.section + 0.5);
    expect(after.scrollWidth).toBeLessThanOrEqual(after.clientWidth + 1);
  });

  test('narrow window: one card per row', async ({ page }) => {
    await page.setViewportSize({ width: 640, height: 900 });
    await page.goto('/');
    await nav(page).click();
    const lefts = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('.ai-section--seen .ai-card')).map((card) => Math.round(card.getBoundingClientRect().left)));
    expect(lefts.length).toBeGreaterThan(1);
    expect(new Set(lefts).size).toBe(1);
    const columns = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('.ai-section__cards')).map((grid) => getComputedStyle(grid).gridTemplateColumns.trim().split(/\s+/).length));
    expect(columns.every((count) => count === 1)).toBe(true);
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
