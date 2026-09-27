import { expect, test } from '@playwright/test';

const cardName = /Project Task：为 reconcile findings/;

test.beforeEach(async ({ page }) => {
  await page.goto('/');
});

test('card body renders Markdown and the popover writes the edited source back', async ({ page }) => {
  const card = page.getByRole('article', { name: cardName });
  const body = card.locator('.task-card__body');
  await body.click();

  const editor = page.getByRole('dialog', { name: '编辑任务正文' });
  const field = editor.getByRole('textbox', { name: '任务正文 Markdown' });
  await expect(field).toBeFocused();
  await expect(field).toHaveValue('为 reconcile findings 补齐 evidence diff 摘要');

  // Pasted line breaks collapse: a task body is a single source line.
  await field.fill('**未完成 · 库存预占**：修复\n`inventory.go` 与 [文档](https://example.com) ~~旧方案~~');
  await expect(field).toHaveValue('**未完成 · 库存预占**：修复 `inventory.go` 与 [文档](https://example.com) ~~旧方案~~');
  await expect(editor.getByLabel('渲染预览').locator('strong')).toHaveText('未完成 · 库存预占');
  await field.press('Enter');
  await expect(editor).toBeHidden();

  // Accessible names carry the plain text: screen readers must not read `**` aloud.
  const updated = page.getByRole('article', {
    name: '未完成 Project Task：未完成 · 库存预占：修复 inventory.go 与 文档 旧方案',
    exact: true,
  });
  await expect(updated.getByRole('button', { name: '完成“未完成 · 库存预占：修复 inventory.go 与 文档 旧方案”', exact: true })).toBeVisible();
  const rendered = updated.locator('.task-card__body');
  await expect(rendered.locator('strong')).toHaveText('未完成 · 库存预占');
  await expect(rendered.locator('code')).toHaveText('inventory.go');
  await expect(rendered.locator('a')).toHaveAttribute('href', 'https://example.com');
  await expect(rendered.locator('del')).toHaveText('旧方案');
  await expect(rendered).not.toContainText('**');
});

test('Escape and 取消 discard edits; an untouched editor closes on outside click', async ({ page }) => {
  const card = page.getByRole('article', { name: cardName });
  await card.locator('.task-card__body').click();
  const editor = page.getByRole('dialog', { name: '编辑任务正文' });
  const field = editor.getByRole('textbox', { name: '任务正文 Markdown' });
  await field.fill('不会被保存');

  // Unsaved text is not thrown away by a stray click.
  await page.locator('.view-heading').first().click({ force: true });
  await expect(editor).toBeVisible();
  await field.press('Escape');
  await expect(editor).toBeHidden();
  await expect(card).toBeFocused();
  await expect(card.locator('.task-card__body')).toHaveText('为 reconcile findings 补齐 evidence diff 摘要');

  await card.press('Enter');
  await expect(field).toHaveValue('为 reconcile findings 补齐 evidence diff 摘要');
  await page.locator('.view-heading').first().click({ force: true });
  await expect(editor).toBeHidden();
});

test('an empty body cannot be saved', async ({ page }) => {
  const card = page.getByRole('article', { name: cardName });
  await card.locator('.task-card__body').click();
  const editor = page.getByRole('dialog', { name: '编辑任务正文' });
  await editor.getByRole('textbox', { name: '任务正文 Markdown' }).fill('   ');
  await expect(editor.getByRole('button', { name: '保存' })).toBeDisabled();
});

test('the editor opens centred in the content region and scales with the window', async ({ page }) => {
  const measure = async () => {
    await page.getByRole('article', { name: cardName }).locator('.task-card__body').click();
    const editor = page.getByRole('dialog', { name: '编辑任务正文' });
    await expect(editor).toBeVisible();
    const box = (await editor.boundingBox())!;
    const main = (await page.locator('main').boundingBox())!;
    const viewport = page.viewportSize()!;
    const regionTop = Math.max(main.y, 0);
    const regionBottom = Math.min(main.y + main.height, viewport.height);
    await editor.getByRole('button', { name: '取消' }).click();
    await expect(editor).toBeHidden();
    return {
      width: box.width,
      height: box.height,
      dx: Math.abs(box.x + box.width / 2 - (main.x + main.width / 2)),
      dy: Math.abs(box.y + box.height / 2 - (regionTop + regionBottom) / 2),
      widthRatio: box.width / main.width,
    };
  };

  await page.setViewportSize({ width: 1180, height: 800 });
  const small = await measure();
  expect(small.dx).toBeLessThan(2);
  expect(small.dy).toBeLessThan(2);
  expect(small.widthRatio).toBeGreaterThan(0.55);

  await page.setViewportSize({ width: 1440, height: 1000 });
  const large = await measure();
  expect(large.dx).toBeLessThan(2);
  expect(large.dy).toBeLessThan(2);
  expect(large.width).toBeGreaterThan(small.width);
  expect(large.height).toBeGreaterThan(small.height);
});
