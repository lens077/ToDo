import { expect, test, type Page } from '@playwright/test';

/**
 * 阅读 against the mock library (`lib/reading.ts`).
 *
 * The fixture holds two articles: one read to 42% with an anchor and one
 * note, one unread. Fetching is simulated, so 添加 only proves the flow; the
 * Rust side covers extraction in its own tests.
 */
const nav = (page: Page) => page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: '阅读' });

async function openReading(page: Page) {
  await page.goto('/');
  await nav(page).click();
  await expect(page.getByRole('heading', { name: '阅读', level: 1 })).toBeVisible();
}

test.describe('阅读', () => {
  test('sits first in the sidebar and counts unfinished articles', async ({ page }) => {
    await page.goto('/');
    const items = page.getByRole('navigation', { name: '主要视图' }).getByRole('button');
    await expect(items.first()).toHaveAccessibleName('总览');
    await expect(items.nth(1)).toHaveAccessibleName('阅读');
    await expect(nav(page)).toContainText('2');
  });

  test('lists share-style cards with progress and filters by state', async ({ page }) => {
    await openReading(page);
    const cards = page.getByTestId('reading-card');
    await expect(cards).toHaveCount(2);
    const partial = cards.filter({ hasText: '可持续的个人知识系统' });
    await expect(partial.getByRole('button', { name: '继续阅读 · 42%' })).toBeVisible();
    await expect(partial.locator('.reading-card__notes')).toContainText('1 条笔记');
    await page.getByRole('group', { name: '按阅读状态筛选' }).getByRole('button', { name: /^进行中/ }).click();
    await expect(cards).toHaveCount(1);
    await page.getByRole('group', { name: '按阅读状态筛选' }).getByRole('button', { name: /^已读/ }).click();
    await expect(page.getByText('这个筛选下没有文章')).toBeVisible();
  });

  test('adds a pasted link and rejects a non-URL', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('要添加的链接');
    await input.fill('not a link');
    await page.locator('.reading-add').getByRole('button', { name: '添加' }).click();
    await expect(page.getByRole('alert')).toContainText('完整的 http/https 链接');
    await input.fill('https://example.org/some-article');
    await page.locator('.reading-add').getByRole('button', { name: '添加' }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(3);
    await expect(page.getByTestId('reading-card').first()).toContainText('example.org');
    await expect(nav(page)).toContainText('3');
  });

  test('reopening restores the last position, offers 从头开始, and shows the marker', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    await expect(page.getByRole('heading', { name: '如何建立一个可持续的个人知识系统', level: 1 })).toBeVisible();
    const banner = page.getByRole('status').filter({ hasText: '已回到上次读到的位置 · 42%' });
    await expect(banner).toBeVisible();
    await expect(page.locator('.reader-body .reader-last-read')).toHaveCount(1);
    const pane = page.locator('.reader-pane');
    await expect.poll(async () => pane.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    await banner.getByRole('button', { name: '从头开始' }).click();
    await expect.poll(async () => pane.evaluate((element) => element.scrollTop)).toBe(0);
    await expect(banner).toHaveCount(0);
  });

  test('stored annotations revive and the note pane locates them', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    const note = page.getByTestId('reader-note');
    await expect(note).toHaveCount(1);
    await expect(note).toContainText('知识不是囤积，而是流动起来');
    await note.getByRole('button', { name: /知识的价值不在于拥有/ }).click();
    await expect(note).toHaveClass(/is-selected/);
  });

  test('selecting text creates a highlight, a note can be written, and the card can be removed', async ({ page }) => {
    await openReading(page);
    await page.getByTestId('reading-card').filter({ hasText: '深度工作' }).getByRole('button', { name: '开始阅读' }).click();
    await expect(page.getByTestId('reader-note')).toHaveCount(0);
    const paragraph = page.locator('.reader-body p').nth(1);
    const box = await paragraph.boundingBox();
    if (!box) throw new Error('paragraph not laid out');
    await page.mouse.move(box.x + 4, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 160, box.y + box.height / 2, { steps: 8 });
    await page.mouse.up();
    const selectionToolbar = page.getByRole('toolbar', { name: '标记操作' });
    await expect(selectionToolbar).toBeVisible();
    await expect(selectionToolbar.getByRole('button', { name: '标记并写笔记' })).toBeVisible();
    await selectionToolbar.getByRole('button', { name: '标记并写笔记' }).click();
    await expect(page.getByTestId('reader-note')).toHaveCount(1);
    await page.getByLabel('笔记内容').fill('这一段值得再读一遍');
    await page.getByRole('button', { name: '保存' }).click();
    await expect(page.getByTestId('reader-note')).toContainText('这一段值得再读一遍');
    await page.getByTestId('reader-note').getByRole('button', { name: '删除' }).click();
    await expect(page.getByTestId('reader-note')).toHaveCount(0);
  });

  test('scrolling to the end marks the article read and the list reflects it', async ({ page }) => {
    await openReading(page);
    await page.getByTestId('reading-card').filter({ hasText: '深度工作' }).getByRole('button', { name: '开始阅读' }).click();
    const pane = page.locator('.reader-pane');
    await pane.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
      element.dispatchEvent(new Event('scroll'));
    });
    await expect(page.locator('.reader__meta')).toContainText('已读 100%');
    await page.getByRole('button', { name: '返回列表' }).click();
    const card = page.getByTestId('reading-card').filter({ hasText: '深度工作' });
    await expect(card.locator('.reading-chip--read')).toHaveText(/已读/);
    await expect(nav(page)).toContainText('1');
  });

  test('标为已读 and 标为未读 round-trip from the reader bar', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    await page.getByRole('button', { name: '标为已读' }).click();
    await expect(page.locator('.reader__meta')).toContainText(/已读$/);
    await page.getByRole('button', { name: '标为未读' }).click();
    await expect(page.getByRole('button', { name: '标为已读' })).toBeVisible();
  });
});

test.describe('阅读上下文', () => {
  async function openFirstArticle(page: Page) {
    await openReading(page);
    await page.getByTestId('reading-card').first().getByRole('button', { name: /继续阅读|开始阅读|重读/ }).click();
    await expect(page.getByTestId('reading-context')).toBeVisible();
  }

  test('saves context, links project/task, and refetch keeps everything', async ({ page }) => {
    await openFirstArticle(page);
    const panel = page.getByTestId('reading-context');
    await page.getByRole('button', { name: '阅读上下文' }).click();

    await page.getByPlaceholder(/为什么保存这篇文章/).fill('研究 E2E 定位方式');
    await page.getByPlaceholder(/以后阅读时重点回答什么问题/).fill('能否兼容 Playwright？');
    await page.getByPlaceholder(/阅读后的理解与结论/).fill('可访问性树方案可行');
    // Blur the last field triggers one save.
    await page.getByPlaceholder(/阅读后的理解与结论/).blur();
    await expect(page.getByText('上下文已保存')).toBeVisible();

    // Link a project and a task by reference ID.
    await page.getByPlaceholder(/输入项目或任务 ID 关联/).fill('proj-1');
    await page.getByRole('button', { name: '关联项目' }).click();
    await expect(panel.locator('.rctx__chip', { hasText: 'proj-1' })).toBeVisible();
    await page.getByPlaceholder(/输入项目或任务 ID 关联/).fill('a1b2c3');
    await page.getByRole('button', { name: '关联任务' }).click();
    await expect(panel.locator('.rctx__chip', { hasText: 'a1b2c3' })).toBeVisible();

    // Re-parse: source refreshes, context/notes/links survive.
    await page.getByRole('button', { name: '重新解析' }).click();
    await expect(page.getByText('已重新解析（上下文与笔记保留）')).toBeVisible();
    await expect(panel.getByPlaceholder(/为什么保存这篇文章/)).toHaveValue('研究 E2E 定位方式');
    await expect(panel.getByPlaceholder(/阅读后的理解与结论/)).toHaveValue('可访问性树方案可行');
    await expect(panel.locator('.rctx__chip', { hasText: 'proj-1' })).toBeVisible();
    await expect(panel.locator('.rctx__chip', { hasText: 'a1b2c3' })).toBeVisible();

    // Export copies a Markdown reference card to the clipboard.
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.getByRole('button', { name: '导出引用' }).click();
    await expect(page.getByText('引用卡片已复制到剪贴板')).toBeVisible();
    const clipboard = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboard).toContain('# 收藏原因');
    expect(clipboard).toContain('研究 E2E 定位方式');
  });
});
