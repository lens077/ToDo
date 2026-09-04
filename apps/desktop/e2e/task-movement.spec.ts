import { expect, test } from '@playwright/test';

const doneTask = /Project Task：确定 TODO\.md v1 的保真写回边界/;
const todoTask = /Project Task：完善 agent hook 安装后的首次使用说明/;

async function firstTaskIn(page: import('@playwright/test').Page, selector: string) {
  return page.locator(selector).getByRole('article').first();
}

test.beforeEach(async ({ page }) => {
  await page.goto('/?policy=normal');
});

test('a completed task can be dragged back to Todo and is reopened at the top', async ({ page }) => {
  const task = page.getByRole('article', { name: doneTask });
  await task.dragTo(page.getByRole('region', { name: '待梳理' }));

  const first = await firstTaskIn(page, '[aria-label="待梳理"]');
  await expect(first).toHaveAccessibleName(/未完成 Project Task：确定 TODO\.md v1/);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('keyboard context menu resumes completed work in Doing at the top', async ({ page }) => {
  const task = page.getByRole('article', { name: doneTask });
  await task.focus();
  await page.keyboard.press('Shift+F10');

  const menu = page.getByRole('menu', { name: /确定 TODO\.md v1.*任务菜单/ });
  await expect(menu.getByRole('menuitem', { name: '继续处理（正在推进）' })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(menu.getByRole('menuitem', { name: '恢复到 待梳理' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toBeHidden();
  await expect(task).toBeFocused();

  await page.keyboard.press('Shift+F10');
  await page.keyboard.press('Tab');
  await expect(menu).toBeHidden();
  await expect(task).toBeFocused();

  await page.keyboard.press('Shift+F10');
  await page.keyboard.press('Enter');

  const first = await firstTaskIn(page, '.focus-grid');
  await expect(first).toHaveAccessibleName(/未完成 Project Task：确定 TODO\.md v1/);
  await expect(first).toBeFocused();
});

test('dropping onto the current column does not reorder the task', async ({ page }) => {
  const first = await firstTaskIn(page, '.focus-grid');
  const name = await first.getAttribute('aria-label');
  await first.dragTo(page.locator('.goal-region--doing'));
  await expect(await firstTaskIn(page, '.focus-grid')).toHaveAttribute('aria-label', name!);
});

test('normal drag into Done bypasses the checkbox safety gate and lands first', async ({ page }) => {
  await page.getByRole('article', { name: todoTask }).dragTo(page.locator('#goal-done-region'));

  await expect(page.getByRole('dialog')).toHaveCount(0);
  const first = await firstTaskIn(page, '.done-list');
  await expect(first).toHaveAccessibleName(/已完成 Project Task：完善 agent hook/);
});

test('normal context-menu completion bypasses the checkbox safety gate', async ({ page }) => {
  const task = page.getByRole('article', { name: todoTask });
  await task.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '标为完成（已完成）' }).click();

  await expect(page.getByRole('dialog')).toHaveCount(0);
  const first = await firstTaskIn(page, '.done-list');
  await expect(first).toHaveAccessibleName(/已完成 Project Task：完善 agent hook/);
});

test('a collapsed Todo rail remains a visible restore target', async ({ page }) => {
  await page.getByRole('button', { name: '收起待办' }).click();
  await page.getByRole('article', { name: doneTask }).dragTo(page.locator('#goal-todo-region'));
  await page.getByRole('button', { name: '展开待办' }).click();

  const first = await firstTaskIn(page, '[aria-label="待梳理"]');
  await expect(first).toHaveAccessibleName(/未完成 Project Task：确定 TODO\.md v1/);
});

test('strict context-menu completion collects an outcome before moving', async ({ page }) => {
  await page.goto('/?policy=strict');
  const task = page.getByRole('article', { name: todoTask });
  await task.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '标为完成（已完成）' }).click();

  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await dialog.getByLabel('完成说明').fill('已通过严格验收');
  await dialog.getByRole('button', { name: '确认完成' }).click();
  await expect(dialog).toBeHidden();

  const first = await firstTaskIn(page, '.done-list');
  await expect(first).toHaveAccessibleName(/已完成 Project Task：完善 agent hook/);
  await expect(first).toContainText('↳ 已通过严格验收');
});

test('choosing partial in a move dialog records progress without moving', async ({ page }) => {
  await page.goto('/?policy=strict');
  const task = page.getByRole('article', { name: todoTask });
  await task.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '标为完成（已完成）' }).click();

  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await dialog.getByRole('radio', { name: /部分完成/ }).check();
  await dialog.getByLabel(/完成比例/).fill('65');
  await dialog.getByLabel(/进度说明/).fill('文档完成，等待演练');
  await dialog.getByRole('button', { name: '确认部分完成' }).click();

  await expect(dialog).toBeHidden();
  const stillTodo = page.locator('[aria-label="待梳理"]').getByRole('article', { name: todoTask });
  await expect(stillTodo).toContainText('65%');
  await expect(stillTodo).toContainText('文档完成，等待演练');
  await expect(stillTodo).toBeFocused();
});

test('a partial task uses the outcome dialog before a move into Done', async ({ page }) => {
  const task = page.getByRole('article', { name: /Project Task：为 reconcile findings/ });
  await task.getByRole('button', { name: /^部分完成“/ }).click();
  await task.getByLabel('完成比例数值').fill('60');
  await task.getByLabel('进度说明').fill('核心完成，仍待验证');
  await task.getByLabel('进度说明').press('Enter');

  await task.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '标为完成（已完成）' }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(task).toHaveAccessibleName(/未完成 Project Task/);
});

test('Move to submenu exposes every real column and always inserts first', async ({ page }) => {
  const task = page.getByRole('article', { name: todoTask });
  await task.click({ button: 'right' });
  const menu = page.getByRole('menu', { name: /完善 agent hook.*任务菜单/ });
  await menu.getByRole('menuitem', { name: '移动到…' }).focus();
  await page.keyboard.press('Enter');

  const submenu = page.getByRole('menu', { name: '移动到列' });
  await expect(submenu.getByRole('menuitem')).toHaveCount(4);
  await submenu.dispatchEvent('wheel', { deltaY: 40 });
  await expect(submenu).toBeVisible();
  await expect(submenu.getByRole('menuitem', { name: '正在推进' })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(submenu.getByRole('menuitem', { name: '等待验证' })).toBeFocused();
  await page.keyboard.press('Enter');

  const first = await firstTaskIn(page, '[aria-label="等待验证"]');
  await expect(first).toHaveAccessibleName(/未完成 Project Task：完善 agent hook/);
});

test('Move to submenu becomes a full-width sheet on a narrow desktop', async ({ page }) => {
  await page.setViewportSize({ width: 600, height: 800 });
  const task = page.getByRole('article', { name: doneTask });
  await task.getByRole('button', { name: /打开“确定 TODO\.md v1.*任务菜单/ }).click();
  await page.getByRole('menuitem', { name: '移动到…' }).click();

  const bounds = await page.getByRole('menu', { name: '移动到列' }).boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.width).toBeGreaterThanOrEqual(500);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(600);
});
