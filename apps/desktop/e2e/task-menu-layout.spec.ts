import { expect, test, type Locator, type Page } from '@playwright/test';

const taskName = '测量 1,000 项 Snapshot 构建预算';

async function openTaskMenu(page: Page, policy: 'normal' | 'strict' = 'normal') {
  await page.goto(`/?policy=${policy}`);
  await page.getByRole('button', { name: '增加同时显示数' }).click();
  const task = page.getByRole('article', { name: `未完成 Project Task：${taskName}` });
  const trigger = task.getByRole('button', { name: `打开“${taskName}”的任务菜单` });
  await trigger.click();
  const menu = page.getByRole('menu', { name: `“${taskName}”的任务菜单`, exact: true });
  return {
    task,
    trigger,
    menu,
    priorityTrigger: menu.getByRole('menuitem', { name: '优先级…', exact: true }),
    moveTrigger: menu.getByRole('menuitem', { name: '移动到…', exact: true }),
    priorityMenu: page.getByRole('menu', { name: '设置优先级', exact: true }),
    moveMenu: page.getByRole('menu', { name: '移动到列', exact: true }),
  };
}

async function expectReachableMenus(page: Page, menu: Locator) {
  const viewport = page.viewportSize()!;
  for (const surface of [menu, ...await menu.getByRole('menu').all()]) {
    const box = await surface.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.y).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width);
    expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height);
  }
  const items = await menu.getByRole('menuitem').evaluateAll((buttons) => buttons.map((button) => {
    const { left, right, top, bottom } = button.getBoundingClientRect();
    return { label: button.textContent, left, right, top, bottom };
  }));
  for (let first = 0; first < items.length; first += 1) {
    for (let second = first + 1; second < items.length; second += 1) {
      const a = items[first];
      const b = items[second];
      const overlaps = Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1
        && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;
      expect(overlaps, `${a.label} must not cover ${b.label}`).toBe(false);
    }
  }
}

test.describe('wide task menu positioning', () => {
  test.use({ viewport: { width: 986, height: 800 } });

  test('Move to submenu keeps its items inside the visible panel when opened left', async ({ page }) => {
    await page.addStyleTag({ content: 'body { overflow: hidden !important; }' });
    const { menu, moveTrigger, moveMenu } = await openTaskMenu(page);
    const rootBoxBeforeOpen = await menu.boundingBox();
    expect(rootBoxBeforeOpen).not.toBeNull();
    const viewport = page.viewportSize()!;
    await menu.evaluate((element, left) => {
      element.style.left = `${left}px`;
    }, viewport.width - rootBoxBeforeOpen!.width - 8);
    await moveTrigger.click();
    await expect(moveMenu).toBeVisible();
    await expect(menu).toHaveClass(/opens-left/);

    const rootBox = await menu.boundingBox();
    const triggerBox = await moveTrigger.boundingBox();
    const panel = await moveMenu.boundingBox();
    expect(rootBox).not.toBeNull();
    expect(triggerBox).not.toBeNull();
    expect(panel).not.toBeNull();
    expect(panel!.x).toBeGreaterThanOrEqual(8);
    expect(panel!.x + panel!.width).toBeLessThanOrEqual(viewport.width - 8);
    expect(panel!.y).toBeGreaterThanOrEqual(8);
    expect(panel!.y + panel!.height).toBeLessThanOrEqual(viewport.height - 7);
    expect(panel!.x + panel!.width).toBeLessThanOrEqual(rootBox!.x);
    expect(Math.abs(panel!.y - triggerBox!.y)).toBeLessThanOrEqual(8);
    for (const item of await moveMenu.getByRole('menuitem').all()) {
      const box = await item.boundingBox();
      expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(panel!.x);
      expect(box!.y).toBeGreaterThanOrEqual(panel!.y);
      expect(box!.x + box!.width).toBeLessThanOrEqual(panel!.x + panel!.width);
      expect(box!.y + box!.height).toBeLessThanOrEqual(panel!.y + panel!.height);
    }
  });
});

for (const width of [600, 680]) {
  test.describe(`task menu at ${width}px`, () => {
    test.use({ viewport: { width, height: 800 } });

    test('priority and Move to stay exclusive across focus, hover, and click', async ({ page }) => {
      const { menu, priorityTrigger, moveTrigger, priorityMenu, moveMenu } = await openTaskMenu(page);
      await priorityTrigger.click();
      await expect(priorityMenu).toBeVisible();
      await moveTrigger.focus();
      await page.keyboard.press('ArrowRight');
      await expect(moveMenu).toBeVisible();
      await expect(menu.getByRole('menu')).toHaveCount(1);
      await expect(priorityMenu).toHaveCount(0);
      await expect(priorityTrigger).toHaveAttribute('aria-expanded', 'false');

      await page.keyboard.press('ArrowLeft');
      await priorityTrigger.click();
      await moveTrigger.hover();
      // Narrow sheets reflow beneath the pointer; hover must not switch them.
      await expect(priorityMenu).toBeVisible();
      await expect(moveMenu).toHaveCount(0);
      await moveTrigger.click();
      await expect(moveMenu).toBeVisible();
      await expect(priorityMenu).toHaveCount(0);
      await expect(menu.getByRole('menu')).toHaveCount(1);
      await expect(moveTrigger).toHaveAttribute('aria-expanded', 'true');
    });

    test('submenu items stay in the viewport without covering root actions', async ({ page }) => {
      const { menu, priorityTrigger, moveTrigger, priorityMenu, moveMenu } = await openTaskMenu(page);
      await priorityTrigger.click();
      await expect(priorityMenu).toBeVisible();
      await expectReachableMenus(page, menu);
      await moveTrigger.click();
      await expect(moveMenu).toBeVisible();
      await expectReachableMenus(page, menu);
    });

    test('arrows traverse one submenu and Escape or Tab restores its opener', async ({ page }) => {
      const { trigger, menu, priorityTrigger, moveTrigger, priorityMenu, moveMenu } = await openTaskMenu(page);
      await priorityTrigger.focus();
      await page.keyboard.press('ArrowRight');
      await expect(priorityMenu.getByRole('menuitem', { name: '!1 最高' })).toBeFocused();
      await page.keyboard.press('ArrowDown');
      await expect(priorityMenu.getByRole('menuitem', { name: '!3 中' })).toBeFocused();
      await page.keyboard.press('ArrowLeft');
      await expect(priorityMenu).toHaveCount(0);
      await expect(priorityTrigger).toBeFocused();
      await page.keyboard.press('ArrowDown');
      await expect(moveTrigger).toBeFocused();
      await page.keyboard.press('ArrowRight');
      await expect(moveMenu.getByRole('menuitem', { name: '待梳理', exact: true })).toBeFocused();
      await page.keyboard.press('ArrowDown');
      await expect(moveMenu.getByRole('menuitem', { name: '等待验证', exact: true })).toBeFocused();
      await page.keyboard.press('ArrowLeft');
      await expect(moveMenu).toHaveCount(0);
      await expect(moveTrigger).toBeFocused();
      await page.keyboard.press('ArrowRight');
      await page.keyboard.press('Escape');
      await expect(menu).toHaveCount(0);
      await expect(trigger).toBeFocused();

      await trigger.press('Enter');
      await priorityTrigger.focus();
      await page.keyboard.press('ArrowRight');
      await page.keyboard.press('Tab');
      await expect(page.getByRole('menu')).toHaveCount(0);
      await expect(trigger).toBeFocused();
    });

    test('selecting a priority removes every menu without reopening Move to', async ({ page }) => {
      const { task, priorityTrigger, priorityMenu } = await openTaskMenu(page);
      await priorityTrigger.click();
      await priorityMenu.getByRole('menuitem', { name: '!3 中' }).click();
      await expect(task.getByLabel('优先级 3', { exact: true })).toBeVisible();
      await expect(page.getByRole('menu')).toHaveCount(0);
      await expect(page.getByRole('dialog')).toHaveCount(0);
    });

    test('strict completion removes the entire menu before its modal takes focus', async ({ page }) => {
      const { task, priorityTrigger, moveTrigger, moveMenu } = await openTaskMenu(page, 'strict');
      await priorityTrigger.click();
      await moveTrigger.focus();
      await page.keyboard.press('ArrowRight');
      await moveMenu.getByRole('menuitem', { name: '已完成', exact: true }).click();
      const dialog = page.getByRole('dialog', { name: '确认任务结局' });
      await expect(dialog).toBeVisible();
      await expect(page.getByRole('menu', { includeHidden: true })).toHaveCount(0);
      await expect(dialog.getByLabel('完成说明')).toBeFocused();
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
      await expect(page.getByRole('menu', { includeHidden: true })).toHaveCount(0);
      await expect(task).toBeVisible();
    });
  });
}
