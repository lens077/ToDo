import { expect, test, type Page } from '@playwright/test';
import { readFile, realpath, writeFile } from 'node:fs/promises';
import { basename, dirname, relative, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import type { DesktopSnapshot } from '../src/lib/desktop';

/** The runner creates a temporary copy; this test never opens the original. */
const url = process.env.E2E_COLUMNS_URL;
const path = process.env.E2E_COLUMNS_PATH;
const dialog = (page: Page) => page.getByRole('dialog', { name: '配置任务列', exact: true });

function body(content: string): string {
  const match = /^(?:\uFEFF)?---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.exec(content);
  if (!match) throw new Error('Expected a managed temporary task file');
  return content.slice(match[0].length);
}

test.describe('live column setup on an isolated copy', () => {
  test.skip(!url || !path, 'E2E_COLUMNS_URL and E2E_COLUMNS_PATH must identify the isolated runner');
  test.use({ baseURL: url });

  test('preview and cancel preserve bytes; confirmed defaults preserve every task until an explicit move', async ({ page, request }) => {
    if (!path) throw new Error('Missing isolated task file');
    const canonical = await realpath(path);
    const temporaryRoot = await realpath(tmpdir());
    const withinTemp = relative(temporaryRoot, canonical);
    if (!withinTemp || withinTemp.startsWith(`..${sep}`) || withinTemp === '..' || resolve(path) !== canonical
      || !basename(dirname(canonical)).startsWith('virgo-columns.') || basename(canonical) !== 'TODO.md') {
      throw new Error('Refusing non-runner path: test requires a canonical virgo-columns.* temporary copy');
    }
    const original = await readFile(canonical, 'utf8');
    const beforeResponse = await request.get('/api/v1/snapshot');
    expect(beforeResponse.ok()).toBe(true);
    const before = await beforeResponse.json() as DesktopSnapshot;
    const task = before.tasks.find((item) => item.id && !item.completed && item.indent_level === 0);
    if (!task?.source_path || !task.id) throw new Error('Fixture needs one top-level incomplete task with source and stable ID');
    const source = task.source_path;
    expect(before.columns.filter((column) => column.source_path === source)).toHaveLength(0);
    const historyUrl = `/api/v1/history?source=${encodeURIComponent(source)}`;
    const historyResponse = await request.get(historyUrl);
    expect(historyResponse.ok()).toBe(true);
    const historyBefore = await historyResponse.json() as unknown[];
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.goto('/');
    const enrollment = page.getByRole('button', { name: '全部录入', exact: true });
    if (await enrollment.isVisible()) await enrollment.click();
    await page.getByRole('combobox', { name: '任务来源', exact: true }).selectOption(source);
    const card = page.getByRole('article', { name: `未完成 Project Task：${task.body}`, exact: true });
    const open = async () => {
      await card.click({ button: 'right' });
      await page.getByRole('menuitem', { name: '配置任务列…', exact: true }).click();
      await expect(dialog(page).getByRole('radio', { name: '创建默认状态列（推荐）', exact: true })).toBeChecked();
      await dialog(page).getByRole('button', { name: '预览差异', exact: true }).click();
      await expect(dialog(page).getByRole('table', { name: '左右对比' })).toBeVisible();
    };
    await open();
    expect(await readFile(canonical, 'utf8') === original, 'preview must not write the temporary source').toBe(true);
    await test.info().attach('live-columns-preview-redacted', {
      body: await dialog(page).screenshot({ path: test.info().outputPath('live-columns-preview-redacted.png'), animations: 'disabled', mask: [dialog(page).locator('.diff-body'), dialog(page).locator('.column-setup-dialog__source')] }),
      contentType: 'image/png',
    });
    await dialog(page).getByRole('button', { name: '取消', exact: true }).click();
    await expect(dialog(page)).toHaveCount(0);
    expect(await readFile(canonical, 'utf8') === original, 'cancel must preserve every byte').toBe(true);
    expect(await (await request.get(historyUrl)).json()).toHaveLength(historyBefore.length);

    await open();
    const proposalResponse = await request.post('/api/v1/sources/columns/preview', {
      data: { source_path: source, expected_project_path: before.project_path, selection: { mode: 'defaults' } },
    });
    expect(proposalResponse.ok()).toBe(true);
    const proposal = await proposalResponse.json() as { proposed: string };
    await dialog(page).getByRole('button', { name: '确认并写入', exact: true }).click();
    await expect(dialog(page)).toHaveCount(0);
    const configured = await readFile(canonical, 'utf8');
    expect(configured === proposal.proposed, 'actual write must equal the backend preview').toBe(true);
    expect(body(configured).startsWith(body(original)), 'the complete original body must remain an unchanged prefix').toBe(true);
    const after = await (await request.get('/api/v1/snapshot')).json() as DesktopSnapshot;
    const preserve = (snapshot: DesktopSnapshot) => snapshot.tasks.map((item) => [item.source_path, item.id, item.body, item.completed, item.indent_level, item.parent_id, item.note]);
    expect(JSON.stringify(preserve(after)) === JSON.stringify(preserve(before)), 'configuration cannot move/reorder or rewrite existing tasks').toBe(true);
    expect(after.columns.filter((column) => column.source_path === source).map((column) => [column.id, column.name, column.done]))
      .toEqual([['todo', '待办', false], ['doing', '进行中', false], ['done', '已完成', true]]);
    expect(await (await request.get(historyUrl)).json()).toHaveLength(historyBefore.length + 1);

    await card.click({ button: 'right' });
    await page.getByRole('menuitem', { name: '移动到…', exact: true }).click();
    const move = page.getByRole('menu', { name: '移动到列', exact: true });
    await expect(move.getByRole('menuitem')).toHaveText(['待办', '进行中', '已完成']);
    await test.info().attach('live-columns-destinations', { body: await move.screenshot({ path: test.info().outputPath('live-columns-destinations.png'), animations: 'disabled' }), contentType: 'image/png' });
    await move.getByRole('menuitem', { name: '进行中', exact: true }).click();
    await expect.poll(async () => {
      const current = await (await request.get('/api/v1/snapshot')).json() as DesktopSnapshot;
      return current.tasks.find((item) => item.id === task.id && item.source_path === source)?.column;
    }).toBe('doing');
    expect(await readFile(canonical, 'utf8') !== configured, 'only explicit movement moves a task').toBe(true);

    // Restore only this already-proven runner-owned copy to exercise the new
    // one-step path independently of the preceding configuration scenario.
    await writeFile(canonical, original);
    await page.reload();
    const unclassified = before.tasks.find((item) => item.id && !item.completed && item.column === null && item.indent_level === 0 && item.id !== task.id)
      ?? before.tasks.find((item) => item.id && !item.completed && item.column === null && item.indent_level === 0);
    if (!unclassified?.id || !unclassified.source_path) throw new Error('Fixture needs an unclassified incomplete task');
    const startCard = page.getByRole('article', { name: `未完成 Project Task：${unclassified.body}`, exact: true });
    const startDialog = page.getByRole('dialog', { name: '创建列并推进任务', exact: true });
    const focus = page.locator('.goal-region--doing');
    const historyBeforeStart = await (await request.get(historyUrl)).json() as unknown[];
    await startCard.dragTo(focus);
    await expect(startDialog.getByRole('table', { name: '左右对比' })).toBeVisible();
    expect(await readFile(canonical, 'utf8') === original, 'direct-start preview must not write').toBe(true);
    await page.keyboard.press('Escape');
    await expect(startDialog).toHaveCount(0);
    expect(await readFile(canonical, 'utf8') === original, 'direct-start cancellation preserves the complete copy').toBe(true);
    expect(await (await request.get(historyUrl)).json()).toHaveLength(historyBeforeStart.length);
    await startCard.dragTo(focus);
    await expect(startDialog.getByRole('table', { name: '左右对比' })).toBeVisible();
    const startProposalResponse = await request.post('/api/v1/sources/columns/preview', {
      data: { source_path: unclassified.source_path, expected_project_path: before.project_path, selection: { mode: 'defaults_and_start', task_id: unclassified.id } },
    });
    expect(startProposalResponse.ok()).toBe(true);
    const startProposal = await startProposalResponse.json() as { proposed: string };
    await test.info().attach('live-columns-direct-start-redacted', {
      body: await startDialog.screenshot({ path: test.info().outputPath('live-columns-direct-start-redacted.png'), animations: 'disabled', mask: [startDialog.locator('.diff-body'), startDialog.locator('.column-setup-dialog__source'), startDialog.locator('.column-setup-dialog__task')] }),
      contentType: 'image/png',
    });
    await startDialog.getByRole('button', { name: '确认并写入', exact: true }).click();
    await expect(startDialog).toHaveCount(0);
    await expect(focus.getByRole('article', { name: `未完成 Project Task：${unclassified.body}`, exact: true })).toBeVisible();
    expect(await readFile(canonical, 'utf8') === startProposal.proposed, 'direct-start must apply the one reviewed atomic edit').toBe(true);
    expect(await (await request.get(historyUrl)).json()).toHaveLength(historyBeforeStart.length + 1);
    const started = await (await request.get('/api/v1/snapshot')).json() as DesktopSnapshot;
    expect(started.tasks.find((item) => item.id === unclassified.id && item.source_path === unclassified.source_path)?.column).toBe('doing');
    expect(started.tasks.find((item) => item.id === unclassified.id && item.source_path === unclassified.source_path)?.completed).toBe(false);
    expect(errors).toEqual([]);
  });
});
