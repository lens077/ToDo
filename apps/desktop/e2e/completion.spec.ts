import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/?policy=strict');
  await page.getByRole('button', { name: /看板/ }).click();
});

async function openBoard(page: import('@playwright/test').Page, policy: 'normal' | 'strict' = 'normal') {
  await page.goto(`/?policy=${policy}`);
  await page.getByRole('button', { name: /看板/ }).click();
}

async function completeTask(page: import('@playwright/test').Page, taskName: RegExp) {
  await page.getByRole('button', { name: taskName }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await dialog.getByLabel('完成说明').fill('验收通过');
  await dialog.getByRole('button', { name: '确认完成' }).click();
  await expect(dialog).toBeHidden();
}

async function openOutcomeMenu(page: import('@playwright/test').Page, taskName: RegExp) {
  const task = page.getByRole('article', { name: taskName });
  await task.locator('summary').filter({ hasText: '···' }).click();
  return page.getByRole('menu', { name: '记录任务结局' });
}

async function recordPartial(page: import('@playwright/test').Page, taskName: RegExp, note: string) {
  const menu = await openOutcomeMenu(page, taskName);
  await menu.getByRole('menuitem', { name: '记录部分完成' }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await dialog.getByLabel(/完成比例/).fill('60');
  await dialog.getByLabel(/进度说明/).fill(note);
  await dialog.getByRole('button', { name: '确认部分完成' }).click();
  await expect(dialog).toBeHidden();
}

async function recordFailure(page: import('@playwright/test').Page, taskName: RegExp, note: string) {
  const menu = await openOutcomeMenu(page, taskName);
  await menu.getByRole('menuitem', { name: '记录失败' }).focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await dialog.getByRole('textbox', { name: '失败原因' }).fill(note);
  await dialog.getByRole('button', { name: '确认失败' }).click();
  await expect(dialog).toBeHidden();
}

const completionTask = /完成“为 reconcile findings/;
const outcomeTask = /Project Task：为 reconcile findings/;

test('strict dialog persists partial progress and its explanation', async ({ page }) => {
  await page.getByRole('button', { name: /完成“为 reconcile findings/ }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });

  await dialog.getByRole('radio', { name: /部分完成/ }).check();
  await dialog.getByLabel(/完成比例/).fill('60');
  await dialog.getByLabel(/进度说明/).fill('核心完成，集成待验证');
  await dialog.getByRole('button', { name: '确认部分完成' }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByText('60%')).toBeVisible();
});

test('strict dialog persists a failed attempt and its explanation', async ({ page }) => {
  await page.getByRole('button', { name: /完成“为 reconcile findings/ }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });

  await dialog.getByRole('radio', { name: /失败/ }).check();
  await dialog.getByRole('textbox', { name: '失败原因' }).fill('负载测试失败');
  await dialog.getByRole('button', { name: '确认失败' }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByLabel(/上次失败于/)).toBeVisible();
});

test('normal policy offers accessible partial and failed outcomes while direct completion keeps its optional note', async ({ page }) => {
  await openBoard(page);

  await page.getByRole('button', { name: completionTask }).click();
  await expect(page.getByRole('dialog', { name: '确认任务结局' })).toBeHidden();
  await expect(page.getByRole('button', { name: /将“为 reconcile findings/ })).toBeVisible();

  await page.reload();
  await page.getByRole('button', { name: /看板/ }).click();
  await recordPartial(page, outcomeTask, '核心完成，集成待验证');
  await expect(page.getByText('60%')).toBeVisible();

  await recordFailure(page, outcomeTask, '负载测试失败');
  await expect(page.getByLabel(/上次失败于/)).toBeVisible();
});

test('mock partial and failed mutations never demote a completed task', async ({ page }) => {
  await openBoard(page, 'strict');
  await completeTask(page, completionTask);

  await recordPartial(page, outcomeTask, '补充记录，不应重开任务');
  await expect(page.getByRole('button', { name: /将“为 reconcile findings/ })).toBeVisible();

  await recordFailure(page, outcomeTask, '失败记录，不应重开任务');
  await expect(page.getByRole('button', { name: /将“为 reconcile findings/ })).toBeVisible();
});
