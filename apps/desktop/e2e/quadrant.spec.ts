import { expect, test } from '@playwright/test';

test.describe('priority quadrant', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: '象限' }).click();
  });

  test('shows four levels and the unranked tray', async ({ page }) => {
    await expect(page.getByRole('heading', { name: '优先级象限' })).toBeVisible();
    for (const name of ['!1 最高', '!2 高', '!3 中', '!4 低', '未定级 0']) {
      await expect(page.getByRole('heading', { name })).toBeVisible();
    }
  });

  test('changes priority from the task menu', async ({ page }) => {
    const card = page.getByRole('article').filter({ hasText: '测量 1,000 项 Snapshot 构建预算' });
    await card.click({ button: 'right' });
    await page.getByRole('menuitem', { name: '优先级…' }).click();
    await page.getByRole('menuitem', { name: '!4 低' }).click();
    await expect(page.getByRole('region', { name: '!4 低' }).getByText('测量 1,000 项 Snapshot 构建预算')).toBeVisible();
  });

  test('number keys update priority without opening the menu', async ({ page }) => {
    const card = page.getByRole('article').filter({ hasText: '测量 1,000 项 Snapshot 构建预算' });
    await card.focus();
    await card.press('1');
    await expect(page.getByRole('region', { name: '!1 最高' }).getByText('测量 1,000 项 Snapshot 构建预算')).toBeVisible();
  });

  test('priority submenu supports ArrowRight and ArrowLeft', async ({ page }) => {
    const card = page.getByRole('article').filter({ hasText: '测量 1,000 项 Snapshot 构建预算' });
    await card.press('Shift+F10');
    const trigger = page.getByRole('menuitem', { name: '优先级…' });
    await trigger.focus();
    await trigger.press('ArrowRight');
    await expect(page.getByRole('menu', { name: '设置优先级' })).toBeVisible();
    await page.getByRole('menuitem', { name: '!1 最高' }).press('ArrowLeft');
    await expect(trigger).toBeFocused();
  });

  test('clears priority into the unranked tray', async ({ page }) => {
    const card = page.getByRole('article').filter({ hasText: '测量 1,000 项 Snapshot 构建预算' });
    await card.click({ button: 'right' });
    await page.getByRole('menuitem', { name: '优先级…' }).click();
    await page.getByRole('menuitem', { name: '清除优先级' }).click();
    await expect(page.getByRole('region', { name: /未定级 1/ }).getByText('测量 1,000 项 Snapshot 构建预算')).toBeVisible();
  });
});
