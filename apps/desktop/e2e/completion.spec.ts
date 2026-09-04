import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/?policy=strict');
});

async function openBoard(page: import('@playwright/test').Page, policy: 'normal' | 'strict' = 'normal') {
  await page.goto(`/?policy=${policy}`);
}

async function completeTask(page: import('@playwright/test').Page, taskName: RegExp) {
  await page.getByRole('button', { name: taskName }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });
  await dialog.getByLabel('完成说明').fill('验收通过');
  await dialog.getByRole('button', { name: '确认完成' }).click();
  await expect(dialog).toBeHidden();
}

// Partial progress and failures are written in the card itself: no dialog
// opens, and the rest of the surface stays readable while the note is typed.
async function recordPartial(page: import('@playwright/test').Page, taskName: RegExp, note: string) {
  const task = page.getByRole('article', { name: taskName });
  await task.getByRole('button', { name: /^部分完成“/ }).click();
  const form = task.getByRole('form', { name: /记录“.*”的部分完成/ });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await form.getByLabel('完成比例数值').fill('60');
  await form.getByLabel('进度说明').fill(note);
  await form.getByLabel('进度说明').press('Enter');
  await expect(form).toBeHidden();
}

async function recordFailure(page: import('@playwright/test').Page, taskName: RegExp, note: string) {
  const task = page.getByRole('article', { name: taskName });
  await task.getByRole('button', { name: /^记录“.*”的失败$/ }).focus();
  await page.keyboard.press('Enter');
  const form = task.getByRole('form', { name: /记录“.*”的失败/ });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await form.getByRole('textbox', { name: '失败原因' }).fill(note);
  await form.getByRole('button', { name: '记录', exact: true }).click();
  await expect(form).toBeHidden();
}

const completionTask = /^完成“为 reconcile findings/;
const outcomeTask = /Project Task：为 reconcile findings/;

test('strict dialog persists partial progress and its explanation', async ({ page }) => {
  await page.getByRole('button', { name: /^完成“为 reconcile findings/ }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });

  await dialog.getByRole('radio', { name: /部分完成/ }).check();
  await dialog.getByLabel(/完成比例/).fill('60');
  await dialog.getByLabel(/进度说明/).fill('核心完成，集成待验证');
  await dialog.getByRole('button', { name: '确认部分完成' }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByText('60%')).toBeVisible();
});

test('strict dialog persists a failed attempt and its explanation', async ({ page }) => {
  await page.getByRole('button', { name: /^完成“为 reconcile findings/ }).click();
  const dialog = page.getByRole('dialog', { name: '确认任务结局' });

  await dialog.getByRole('radio', { name: /失败/ }).check();
  await dialog.getByRole('textbox', { name: '失败原因' }).fill('负载测试失败');
  await dialog.getByRole('button', { name: '确认失败' }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByLabel(/上次失败于/)).toBeVisible();
});

test('normal policy gates a tick behind Enter, and Escape leaves the task open', async ({ page }) => {
  await openBoard(page);

  await page.getByRole('button', { name: completionTask }).click();
  const gate = page.getByRole('dialog', { name: '标为完成？' });
  await expect(gate).toContainText('为 reconcile findings');
  await page.keyboard.press('Escape');
  await expect(gate).toBeHidden();
  await expect(page.getByRole('button', { name: completionTask })).toBeVisible();

  await page.getByRole('button', { name: completionTask }).click();
  await page.keyboard.press('Enter');
  await expect(gate).toBeHidden();
  await expect(page.getByRole('dialog', { name: '确认任务结局' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /将“为 reconcile findings/ })).toBeVisible();
});

test('the completion gate can be switched off in settings and the choice survives reload', async ({ page }) => {
  await openBoard(page);

  await page.getByRole('button', { name: '设置', exact: true }).click();
  const settings = page.getByRole('dialog', { name: '设置' });
  const toggle = settings.getByRole('switch', { name: '勾选完成前先确认' });
  await expect(toggle).toBeChecked();
  await toggle.click();
  await expect(toggle).not.toBeChecked();
  await page.keyboard.press('Escape');
  await expect(settings).toBeHidden();

  await page.getByRole('button', { name: completionTask }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /将“为 reconcile findings/ })).toBeVisible();

  await page.reload();
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await expect(page.getByRole('switch', { name: '勾选完成前先确认' })).not.toBeChecked();
});

test('normal policy offers accessible partial and failed outcomes in the card', async ({ page }) => {
  await openBoard(page);

  await page.reload();
  await recordPartial(page, outcomeTask, '核心完成，集成待验证');
  await expect(page.getByText('60%')).toBeVisible();
  await expect(page.getByText('↳ 核心完成，集成待验证')).toBeVisible();

  await recordFailure(page, outcomeTask, '负载测试失败');
  await expect(page.getByLabel(/上次失败于/)).toBeVisible();
});

test('a completed task offers no partial or failed outcome to record', async ({ page }) => {
  await openBoard(page, 'strict');
  await completeTask(page, completionTask);

  const task = page.getByRole('article', { name: outcomeTask });
  await expect(task.getByRole('button', { name: /将“为 reconcile findings/ })).toBeVisible();
  await expect(task.getByRole('button', { name: /^部分完成“/ })).toHaveCount(0);
  await expect(task.getByRole('button', { name: /^记录“.*”的失败$/ })).toHaveCount(0);
});

test('Escape closes the inline outcome without writing', async ({ page }) => {
  await openBoard(page);
  const task = page.getByRole('article', { name: outcomeTask });
  await task.getByRole('button', { name: /^部分完成“/ }).click();
  const form = task.getByRole('form', { name: /部分完成/ });
  await form.getByLabel('进度说明').fill('还没写完');
  await form.getByLabel('进度说明').press('Escape');
  await expect(form).toBeHidden();
  await expect(page.getByText('↳ 还没写完')).toHaveCount(0);
});
