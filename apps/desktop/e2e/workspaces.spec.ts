import { expect, test, type Page } from '@playwright/test';

/**
 * 工作空间 against the mock registry (`lib/workspace.ts`).
 *
 * The mock keeps the registry's real rules: adding the same path twice never
 * duplicates a project, removing from a workspace keeps the record, and
 * deleting the registry entry is a separate confirmed action. The Rust side
 * covers canonical-path dedup and identity-stable relinking in its own tests.
 */
const nav = (page: Page) => page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: '工作空间' });

test.describe('工作空间', () => {
  test('creates a default workspace and registers two distinct projects', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    await expect(page.getByRole('heading', { name: '工作空间', level: 2 })).toBeVisible();
    await expect(page.getByRole('heading', { name: '默认工作空间', level: 3 })).toBeVisible();

    for (let i = 0; i < 2; i += 1) {
      await page.getByRole('button', { name: '添加项目' }).click();
    }
    const cards = page.locator('.ws-project');
    await expect(cards).toHaveCount(2);
    await expect(cards.first().locator('.ws-badge--reachable')).toHaveText('正常');
    // Distinct directories produce distinct stable IDs.
    const firstId = await cards.first().locator('code').innerText();
    const secondId = await cards.nth(1).locator('code').innerText();
    expect(firstId).not.toBe(secondId);
  });

  test('removing from a workspace keeps the other project; registry delete needs confirm', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    await page.getByRole('button', { name: '添加项目' }).click();
    await page.getByRole('button', { name: '添加项目' }).click();
    const cards = page.locator('.ws-project');
    await expect(cards).toHaveCount(2);

    await cards.first().locator('button[title*="从该工作空间移除"]').click();
    await expect(cards).toHaveCount(1);

    page.once('dialog', (dialog) => dialog.accept());
    await cards.first().locator('button[title*="从注册表删除"]').click();
    await expect(page.getByText('还没有项目')).toBeVisible();
  });

  test('creating a named workspace starts empty', async ({ page }) => {
    await page.goto('/');
    await nav(page).click();
    await page.getByRole('button', { name: '新建工作空间' }).click();
    await page.getByPlaceholder(/工作空间名称/).fill('Client Work');
    await page.getByRole('button', { name: '创建', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Client Work', level: 3 })).toBeVisible();
    const sections = page.locator('.ws-workspace', { hasText: 'Client Work' });
    await expect(sections.getByText('还没有项目')).toBeVisible();
  });
});

test.describe('项目档案与环境', () => {
  async function openDetail(page: Page) {
    await page.goto('/');
    await nav(page).click();
    await page.getByRole('button', { name: '添加项目' }).click();
    await page.getByRole('button', { name: /档案/ }).click();
    await expect(page.getByTestId('project-detail')).toBeVisible();
  }

  test('saves profile fields and keeps them after switching views', async ({ page }) => {
    await openDetail(page);
    await page.getByPlaceholder('这个项目要解决什么问题？').fill('本地任务对账器');
    await page.getByPlaceholder('明确的下一步动作').fill('Phase 2 收尾');
    // Blur triggers save; leave the view (mock state survives a view switch,
    // which is what the Tauri store guarantees across launches) and come back.
    await page.getByPlaceholder('这个项目要解决什么问题？').blur();
    await expect(page.getByText('档案已保存')).toBeVisible();
    await page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: '阅读' }).click();
    await nav(page).click();
    await page.getByRole('button', { name: /档案/ }).click();
    await expect(page.getByTestId('project-detail')).toBeVisible();
    await expect(page.getByPlaceholder('这个项目要解决什么问题？')).toHaveValue('本地任务对账器');
    await expect(page.getByPlaceholder('明确的下一步动作')).toHaveValue('Phase 2 收尾');
  });

  test('environment scan keeps the user declaration', async ({ page }) => {
    await openDetail(page);
    await page.locator('.ws-env__declared').fill('{"node":"22"}');
    await page.getByRole('button', { name: '确认声明' }).click();
    await expect(page.getByText('声明环境已确认')).toBeVisible();

    await page.getByRole('button', { name: '扫描环境' }).click();
    await expect(page.getByText('扫描完成（已保留声明环境）')).toBeVisible();
    // Observed side fills in, declared side is untouched.
    await expect(page.getByText('包管理器：pnpm')).toBeVisible();
    await expect(page.locator('.ws-env__declared')).toHaveValue('{"node":"22"}');
    // Environment variable values never appear; names only.
    await expect(page.getByText('环境变量（仅名称）：DATABASE_URL')).toBeVisible();
  });
});

test.describe('知识文档', () => {
  async function openDetail(page: Page) {
    await page.goto('/');
    await nav(page).click();
    await page.getByRole('button', { name: '添加项目' }).click();
    await page.getByRole('button', { name: /档案/ }).click();
    await expect(page.getByTestId('project-detail')).toBeVisible();
  }

  test('scan suggests types; confirm overrides; reader renders sanitized HTML', async ({ page }) => {
    await openDetail(page);
    await page.getByRole('button', { name: '扫描文档' }).click();
    await expect(page.getByText(/文档扫描完成：3 个文件/)).toBeVisible();
    const docs = page.getByTestId('knowledge-docs').locator('.ws-doc');
    await expect(docs).toHaveCount(3);

    // Suggestion from the filename rule.
    await expect(docs.filter({ hasText: 'README.md' }).locator('select')).toHaveValue('brief');
    await expect(docs.filter({ hasText: '0003-storage.md' }).locator('select')).toHaveValue('decision');

    // User override wins and flips the badge.
    await docs.filter({ hasText: 'notes.md' }).locator('select').selectOption('reference');
    await expect(docs.filter({ hasText: 'notes.md' }).locator('.ws-badge', { hasText: '已确认' })).toBeVisible();

    // Reader renders the document; no script execution, HTML is a projection.
    await docs.filter({ hasText: 'README.md' }).locator('.ws-doc__open').click();
    const reader = page.getByTestId('knowledge-reader');
    await expect(reader).toBeVisible();

    // Full-text search (M4): the mock matches title/body substrings and marks
    // the term; an empty query clears the hits.
    const search = page.getByTestId('knowledge-search');
    await search.getByRole('textbox').fill('SQLite');
    await search.getByRole('button', { name: '搜索' }).click();
    const hits = page.getByTestId('knowledge-search-hits').locator('.ws-doc');
    await expect(hits).toHaveCount(1);
    await expect(hits.first()).toContainText('0003-storage.md');
    await expect(hits.first()).toContainText('【SQLite】');
    // A whitespace-only query clears the hits (button is disabled; Enter runs
    // the same handler). The panel shows the empty-state text again.
    await search.getByRole('textbox').fill('  ');
    await search.getByRole('textbox').press('Enter');
    await expect(page.getByTestId('knowledge-search-hits')).toHaveCount(0);
    await expect(page.getByText('没有匹配的文档。')).toBeVisible();
    await expect(reader.locator('h3')).toHaveText('演示项目');
    await expect(page.getByText('HTML 由 Markdown 重建，不落盘')).toBeVisible();
  });
});

test.describe('开发会话', () => {
  async function openDetail(page: Page) {
    await page.goto('/');
    await nav(page).click();
    await page.getByRole('button', { name: '添加项目' }).click();
    await page.getByRole('button', { name: /档案/ }).click();
    await expect(page.getByTestId('project-detail')).toBeVisible();
  }

  test('begin → finish → resume keeps next actions across view switches', async ({ page }) => {
    await openDetail(page);
    await page.getByPlaceholder(/本次会话目标/).fill('接入 watcher 增量扫描');
    await page.getByRole('button', { name: '开始会话' }).click();
    await expect(page.getByText('会话已开始')).toBeVisible();
    await expect(page.getByTestId('session-resume')).toContainText('进行中');
    // Refusing a second active session is the contract.
    await expect(page.getByPlaceholder(/本次会话目标/)).toHaveCount(0);

    const form = page.locator('.ws-session__form');
    // Textareas: completed, decisions, unresolved, next actions, context.
    await form.locator('textarea').nth(0).fill('watcher 事件接入');
    // Inputs: validation command, result, evidence, linked task IDs.
    await form.locator('input').nth(0).fill('make check-quick');
    await form.locator('input').nth(1).fill('绿');
    await form.locator('textarea').nth(3).fill('补 Session E2E');
    await form.locator('input').nth(3).fill('m0ckwatch, m0ckgone');
    await page.getByRole('button', { name: '完成会话' }).click();
    await expect(page.getByText('会话总结已写入 .virgo/sessions/')).toBeVisible();
    await expect(page.getByTestId('session-resume')).toContainText('已完成');
    await expect(page.getByTestId('session-resume')).toContainText('待续：补 Session E2E');
    // Task links resolve against the (mock) task index; unknown IDs say so.
    await expect(page.getByTestId('session-resume')).toContainText('m0ckwatch');
    await expect(page.getByTestId('session-resume')).toContainText('○ 未完成');
    await expect(page.getByTestId('session-resume')).toContainText('未在根 TODO.md 找到');

    // Switching views and back keeps the session history (mock mirrors the
    // on-disk store that survives restarts).
    await page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: '阅读' }).click();
    await nav(page).click();
    await page.getByRole('button', { name: /档案/ }).click();
    await expect(page.getByTestId('session-list').locator('.ws-doc')).toHaveCount(1);
    await expect(page.getByTestId('session-resume')).toContainText('待续：补 Session E2E');
  });
});
