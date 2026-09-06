import { expect, test } from '@playwright/test';

const tasks = [
  { id: 'parent1', body: 'Grouped parent', column: 'todo', indent_level: 0, parent_id: null, priority: 2 },
  { id: 'child01', body: 'Nested child A', column: 'todo', indent_level: 1, parent_id: 'parent1', priority: 3 },
  { id: 'grand01', body: 'Nested grandchild', column: 'todo', indent_level: 2, parent_id: 'child01', priority: 4 },
  { id: 'child02', body: 'Nested child B', column: 'todo', indent_level: 1, parent_id: 'parent1', priority: null },
  { id: 'single1', body: 'Independent task', column: 'todo', indent_level: 0, parent_id: null, priority: 1 },
].map((task) => ({ ...task, completed: false, due: null, tags: [], progress: null, failed_on: null, note: null }));

const fixture = {
  project_path: '/fixtures/grouped/TODO.md',
  revision: 'a1a1a1a1a1a1a1a1',
  completion_policy: 'normal',
  columns: [
    { id: 'todo', name: 'Todo', done: false },
    { id: 'done', name: 'Done', done: true },
  ],
  tasks,
  today: { overdue: [], today: [], upcoming: [], next: tasks },
};

test.beforeEach(async ({ page }) => {
  await page.route('**/api/v1/health', (route) => route.fulfill({ json: { ok: true, read_only: false } }));
  await page.route('**/api/v1/snapshot', (route) => route.fulfill({ json: fixture }));
  await page.goto('/');
  await page.getByRole('button', { name: '象限', exact: true }).click();
});

test('mixed-priority children and grandchildren stay visibly nested under one draggable parent', async ({ page }) => {
  const group = page.locator('[data-group-root="parent1"]');
  await expect(page.locator('[data-group-root]')).toHaveCount(2);
  await expect(page.getByRole('region', { name: '!2 高' }).locator('[data-group-root="parent1"]')).toBeVisible();
  for (const id of ['child01', 'grand01', 'child02']) {
    await expect(group.locator(`[data-task-id="${id}"]`)).toBeVisible();
    await expect(group.locator(`[data-task-id="${id}"]`)).toHaveAttribute('draggable', 'false');
  }
  await expect(group.locator('[data-task-id="parent1"]')).toHaveAttribute('draggable', 'true');
  await expect(page.getByRole('region', { name: '!3 中' }).locator('[data-task-id]')).toHaveCount(0);
  await expect(page.getByRole('region', { name: '!4 低' }).locator('[data-task-id]')).toHaveCount(0);
  await expect(page.getByRole('region', { name: /未定级/ }).locator('[data-task-id]')).toHaveCount(0);
});

test('narrow desktop windows retain native HTML drag capability', async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 950 });
  const parent = page.locator('[data-task-id="parent1"]');
  await expect(parent).toHaveAttribute('draggable', 'true');
  expect(await parent.evaluate((node) => getComputedStyle(node).getPropertyValue('-webkit-user-drag'))).toBe('element');
});

test('a matching descendant keeps its entire parent group visible under a date filter', async ({ page }) => {
  const datedChild = { ...tasks[1], due: '2026-08-01' };
  const datedFixture = { ...fixture, tasks: tasks.map((task) => task.id === 'child01' ? datedChild : task), today: { overdue: [datedChild], today: [], upcoming: [], next: [] } };
  await page.route('**/api/v1/snapshot', (route) => route.fulfill({ json: datedFixture }));
  await page.reload();
  await page.getByRole('button', { name: '象限', exact: true }).click();
  await page.getByRole('button', { name: /逾期/ }).click();
  await expect(page.locator('[data-group-root]')).toHaveCount(1);
  await expect(page.locator('[data-group-root="parent1"] [data-task-id="child02"]')).toBeVisible();
  await expect(page.locator('[data-group-root="parent1"] [data-task-id="grand01"]')).toBeVisible();
});

test('a completed parent remains as context while a descendant is still open', async ({ page }) => {
  await page.route('**/api/v1/snapshot', (route) => route.fulfill({ json: {
    ...fixture,
    tasks: tasks.map((task) => task.id === 'parent1' ? { ...task, completed: true } : task),
  } }));
  await page.reload();
  await page.getByRole('button', { name: '象限', exact: true }).click();
  const group = page.locator('[data-group-root="parent1"]');
  await expect(group.locator('[data-task-id="parent1"]')).toHaveClass(/is-complete/);
  await expect(group.locator('[data-task-id="child01"]')).toBeVisible();
});

test('a group priority menu sends exactly one subtree write and closes', async ({ page }) => {
  const writes: unknown[] = [];
  await page.route('**/api/v1/tasks/parent1/priority', async (route) => {
    writes.push(route.request().postDataJSON());
    await route.fulfill({ json: {
      ...fixture,
      revision: 'b2b2b2b2b2b2b2b2',
      tasks: tasks.map((task) => task.id === 'single1' ? task : { ...task, priority: 1 }),
    } });
  });
  await page.setViewportSize({ width: 640, height: 950 });
  const parent = page.locator('[data-task-id="parent1"]');
  await parent.getByRole('button', { name: '打开“Grouped parent”的任务菜单', exact: true }).click();
  await page.getByRole('menuitem', { name: '优先级…', exact: true }).click();
  await page.getByRole('menuitem', { name: /^!1 最高/ }).click();
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('region', { name: '!1 最高' }).locator('[data-group-root="parent1"]')).toBeVisible();
  expect(writes).toEqual([{ priority: 1, scope: 'subtree' }]);
});
