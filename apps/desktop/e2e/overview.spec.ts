import { expect, test, type Page } from '@playwright/test';

/**
 * 总览 (Phase 7): the cross-project dashboard, driven through the mock
 * registry. Creating two projects here exercises the same fixtures the
 * workspace specs use; the dashboard must answer status + next step without
 * opening any project.
 */
const overviewNav = (page: Page) =>
  page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: '总览' });

async function seedTwoProjects(page: Page) {
  await page.goto('/');
  await page
    .getByRole('navigation', { name: '主要视图' })
    .getByRole('button', { name: '工作空间' })
    .click();
  await page.getByRole('button', { name: '添加项目' }).click();
  await page.getByRole('button', { name: '添加项目' }).click();
}

test.describe('总览', () => {
  test('totals and per-project cards answer status and next step', async ({ page }) => {
    await seedTwoProjects(page);
    await overviewNav(page).click();
    const totals = page.getByTestId('overview-totals');
    await expect(totals).toContainText('项目');

    // Two cards, both reachable, one marked as the current lifecycle default.
    const cards = page.getByTestId('overview-card');
    await expect(cards).toHaveCount(2);
    await expect(cards.first().locator('.ws-badge--reachable').first()).toHaveText('正常');
    // No profile yet: placeholders are explicit, not blank.
    await expect(cards.first().getByText('里程碑')).toBeVisible();
    await expect(cards.first().locator('dd').first()).toHaveText('—');

    // Filters narrow the list without error.
    await page.getByRole('group', { name: '按状态筛选' }).getByRole('button', { name: '有进行中会话' }).click();
    await expect(page.getByText('这个筛选下没有项目。')).toBeVisible();
    await page.getByRole('group', { name: '按状态筛选' }).getByRole('button', { name: '全部' }).click();
    await expect(cards).toHaveCount(2);
  });

  test('empty registry shows a pointing empty state', async ({ page }) => {
    await page.goto('/');
    await overviewNav(page).click();
    // First load auto-creates nothing here: the overview mock has no projects
    // until 工作空间 seeds them, so the empty state must say where to go.
    await expect(page.getByText(/还没有注册项目|这个筛选下没有项目/)).toBeVisible();
  });
});
