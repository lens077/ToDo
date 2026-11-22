import { expect, test, type Page } from '@playwright/test';

/**
 * 阅读 against the mock library (`lib/reading.ts`).
 *
 * The fixture holds two articles: one read to 42% with an anchor and one
 * note, one unread. Fetching is simulated, so 添加 only proves the flow; the
 * Rust side covers extraction in its own tests.
 */
const nav = (page: Page) => page.getByRole('navigation', { name: '主要视图' }).getByRole('button', { name: '收件箱' });

async function openReading(page: Page) {
  await page.goto('/');
  await nav(page).click();
  await expect(page.getByRole('heading', { name: '收件箱', level: 1 })).toBeVisible();
}

test.describe('阅读', () => {
  test('sits first in the sidebar and counts unfinished articles', async ({ page }) => {
    await page.goto('/');
    const items = page.getByRole('navigation', { name: '主要视图' }).getByRole('button');
    await expect(items.first()).toHaveAccessibleName('总览');
    await expect(items.nth(1)).toHaveAccessibleName('收件箱');
    await expect(nav(page)).toContainText('2');
  });

  test('lists share-style cards with progress and filters by state', async ({ page }) => {
    await openReading(page);
    const cards = page.getByTestId('reading-card');
    await expect(cards).toHaveCount(2);
    const partial = cards.filter({ hasText: '可持续的个人知识系统' });
    await expect(partial.getByRole('button', { name: '继续阅读 · 42%' })).toBeVisible();
    await expect(partial.locator('.reading-card__notes')).toContainText('1 处标记');
    await page.getByRole('group', { name: '按阅读状态筛选' }).getByRole('button', { name: /^进行中/ }).click();
    await expect(cards).toHaveCount(1);
    await page.getByRole('group', { name: '按阅读状态筛选' }).getByRole('button', { name: /^已读/ }).click();
    await expect(page.getByText('这个筛选下没有内容')).toBeVisible();
  });

  test('adds a pasted link through the unified composer', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('https://example.org/some-article');
    await page.locator('.inbox-add').getByRole('button', { name: '添加文章' }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(3);
    await expect(page.getByTestId('reading-card').first()).toContainText('example.org');
    await expect(nav(page)).toContainText('3');
  });

  test('typing text stores a note and the kind filter isolates it', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    // A′ 分流：非链接文本 → 随记，Markdown 直存、标题从正文派生。
    await input.fill('地铁上想到的：发布节奏改成两周一版');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    const noteCard = page.getByTestId('reading-card').filter({ hasText: '地铁上想到的' });
    await expect(noteCard).toHaveCount(1);
    await expect(noteCard).toContainText('随记');
    await expect(input).toHaveValue('');
    // 类型维与状态维独立过滤。
    await page.getByRole('group', { name: '按类型筛选' }).getByRole('button', { name: /^随记/ }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(1);
    await page.getByRole('group', { name: '按类型筛选' }).getByRole('button', { name: /^文章/ }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(2);
    await page.getByRole('group', { name: '按类型筛选' }).getByRole('button', { name: /^全部/ }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(3);
  });

  test('note drawer edits markdown and marks the card 已编辑', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('## 草稿\n\n- [ ] 第一件事');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    await page.getByTestId('reading-card').filter({ hasText: '草稿' }).locator('.reading-card__open').click();
    const drawer = page.getByRole('dialog', { name: '编辑随记' });
    await expect(drawer).toBeVisible();
    const body = drawer.getByRole('textbox', { name: '随记正文（Markdown）' });
    await expect(body).toHaveValue(/## 草稿/);
    await body.fill('## 草稿改\n\n- [x] 第一件事');
    await drawer.getByRole('button', { name: '保存' }).click();
    await expect(drawer).toHaveCount(0);
    const noteCard = page.getByTestId('reading-card').filter({ hasText: '草稿改' });
    await expect(noteCard).toContainText('已编辑');
  });

  test('pasting several links at once queues each of them', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    // 多行粘贴在单行输入框里会被折成空格，这曾经拼成一条永远抓不到的坏链接。
    await input.fill('https://example.net/a https://example.net/b');
    await page.locator('.inbox-add').getByRole('button', { name: '添加文章' }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(4);
    await expect(page.getByTestId('reading-card').filter({ hasText: 'example.net' })).toHaveCount(2);
    await expect(input).toHaveValue('');
  });

  test('composer hint follows the detected type', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    const hint = page.locator('.inbox-add .inbox-hint');
    const card = page.locator('.inbox-add');
    // 空态：hint 隐藏（placeholder 承担提示）。
    await expect(hint).toBeHidden();
    // 文本态：紫缘 + 随记 hint + 存入随记按钮。
    await input.fill('随便记点什么');
    await expect(card).toHaveClass(/is-note/);
    await expect(hint).toContainText('直接打字 → 存入随记');
    // 链接态：teal 缘 + 文章 hint + 添加文章按钮。
    await input.fill('https://example.org/hello');
    await expect(card).toHaveClass(/is-link/);
    await expect(hint).toContainText('检测到链接 → 存为文章');
    await expect(page.locator('.inbox-add').getByRole('button', { name: '添加文章' })).toBeVisible();
  });

  test('type and state filters stack and the empty state explains itself', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('一条待读的随记');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    const kinds = page.getByRole('group', { name: '按类型筛选' });
    const states = page.getByRole('group', { name: '按阅读状态筛选' });
    // 类型维单独过滤。
    await kinds.getByRole('button', { name: /^随记/ }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(1);
    // 类型 × 状态叠加：随记都是未读，已读态下没有条目，空态要说明原因。
    await states.getByRole('button', { name: /^已读/ }).click();
    await expect(page.getByText('这个筛选下没有内容')).toBeVisible();
    await states.getByRole('button', { name: /^全部/ }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(1);
    await kinds.getByRole('button', { name: /^全部/ }).click();
    await expect(page.getByTestId('reading-card')).toHaveCount(3);
  });

  test('note drawer inserts markdown at the cursor and Escape closes it', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('正文开头');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    await page.getByTestId('reading-card').filter({ hasText: '正文开头' }).locator('.reading-card__open').click();
    const body = page.getByRole('textbox', { name: '随记正文（Markdown）' });
    // 光标在文本末尾：工具条把任务语法插进光标处。
    await body.click();
    await page.getByRole('dialog', { name: '编辑随记' }).getByRole('button', { name: '☑ 任务' }).click();
    await expect(body).toHaveValue(/正文开头\n- \[ \] /);
    // Escape 是 dialog 原生关闭：未保存的插入被丢弃（显式保存才落库）。
    await body.press('Escape');
    await expect(page.getByRole('dialog', { name: '编辑随记' })).toHaveCount(0);
    await page.getByTestId('reading-card').filter({ hasText: '正文开头' }).locator('.reading-card__open').click();
    await expect(body).toHaveValue('正文开头');
    await page.getByRole('dialog', { name: '编辑随记' }).getByRole('button', { name: '关闭' }).click();
    await expect(page.getByRole('dialog', { name: '编辑随记' })).toHaveCount(0);
  });

  test('ControlOrMeta+Enter saves the note from the drawer', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('快捷键保存的随记');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    await page.getByTestId('reading-card').filter({ hasText: '快捷键保存的随记' }).locator('.reading-card__open').click();
    const body = page.getByRole('textbox', { name: '随记正文（Markdown）' });
    await body.fill('快捷键保存的随记（改）');
    await body.press('ControlOrMeta+Enter');
    await expect(page.getByRole('dialog', { name: '编辑随记' })).toHaveCount(0);
    await expect(page.getByTestId('reading-card').filter({ hasText: '快捷键保存的随记（改）' })).toHaveCount(1);
  });

  test('note delete asks for confirmation', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('会被确认的随记');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    // 先拒绝：卡片必须还在。
    page.once('dialog', (dialog) => dialog.dismiss());
    await page.getByTestId('reading-card').filter({ hasText: '会被确认的随记' }).getByLabel(/删除随记/).click();
    await expect(page.getByTestId('reading-card').filter({ hasText: '会被确认的随记' })).toHaveCount(1);
    // 再接受：卡片移除。
    page.once('dialog', (dialog) => dialog.accept());
    await page.getByTestId('reading-card').filter({ hasText: '会被确认的随记' }).getByLabel(/删除随记/).click();
    await expect(page.getByTestId('reading-card').filter({ hasText: '会被确认的随记' })).toHaveCount(0);
  });

  test('triage turns an inbox note into a task and archives the original', async ({ page }) => {
    await openReading(page);
    const input = page.getByLabel('收件箱输入：链接存文章，文本记随记');
    await input.fill('分拣闭环的随记');
    await page.locator('.inbox-add').getByRole('button', { name: '存入随记' }).click();
    // 入口：收件箱工具行（随记视图已移出导航）。点击会切视图并卸载按钮，
    // 用 dispatchEvent 绕开 actionability 重试。
    await page.getByRole('button', { name: /随记分拣/ }).dispatchEvent('click');
    await expect(page.getByRole('heading', { name: /随记/ })).toBeVisible();
    const ledger = page.locator('.capture-ledger');
    const row = ledger.locator('.capture-item').filter({ hasText: '分拣闭环的随记' });
    await expect(row).toHaveCount(1);
    // 真实鼠标派发在该场景会挂起（行卸载时序），与「随记分拣」入口同法用
    // dispatchEvent；真机人工验收覆盖真实点击。
    await row.getByRole('button', { name: /分拣为任务/ }).dispatchEvent('click');
    // 原话留档：分拣列表不再显示，但收件箱仍有这条随记。
    await expect(ledger.locator('.capture-item')).toHaveCount(0);
    await nav(page).click();
    await expect(page.getByTestId('reading-card').filter({ hasText: '分拣闭环的随记' })).toHaveCount(1);
  });

  test('reader image opens a lightbox and outside click closes it', async ({ page }) => {
    await openReading(page);
    await page.getByTestId('reading-card').filter({ hasText: '可持续的个人知识系统' }).locator('.reading-card__open').click();
    await expect(page.getByTestId('reader-body')).toBeVisible();
    const img = page.locator('.reader-body img').first();
    await expect(img).toBeVisible();
    // 点击图片 → 全屏放大
    await img.click();
    const lightbox = page.locator('.img-lightbox');
    await expect(lightbox).toBeVisible();
    await expect(lightbox.locator('img')).toHaveAttribute('src', /data:image/);
    // 点击图片本身：不关闭（pointerup 双击检测单独计次，不触发关闭）
    await lightbox.locator('img').dispatchEvent('click');
    await expect(lightbox).toBeVisible();
    // 点击图片外区域：关闭
    await lightbox.dispatchEvent('click');
    await expect(lightbox).toHaveCount(0);
    // 重开一次验证 Esc 关闭
    await img.click();
    await expect(lightbox).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(lightbox).toHaveCount(0);
  });

  test('lightbox outside-close can be turned off in 设置', async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.setItem('todo.app-settings.v1', JSON.stringify({
        confirmCompletion: true,
        density: 'normal',
        openSession: 'reuse',
        usageAnalytics: true,
        clipboardImport: true,
        lightboxOutsideClose: false,
      }));
    });
    await openReading(page);
    await page.getByTestId('reading-card').filter({ hasText: '可持续的个人知识系统' }).locator('.reading-card__open').click();
    const img = page.locator('.reader-body img').first();
    await expect(img).toBeVisible();
    await img.click();
    const lightbox = page.locator('.img-lightbox');
    await expect(lightbox).toBeVisible();
    await expect(lightbox).toHaveAttribute('data-outside-close', 'off');
    // outside-close 关闭后：点图片外区域不再关闭，只能 ✕ 关。
    await lightbox.dispatchEvent('click');
    await expect(lightbox).toBeVisible();
    await lightbox.getByRole('button', { name: '关闭图片预览' }).dispatchEvent('click');
    await expect(lightbox).toHaveCount(0);
  });

  test('sidebar no longer lists the standalone capture view', async ({ page }) => {
    await page.goto('/');
    const items = page.getByRole('navigation', { name: '主要视图' }).getByRole('button');
    await expect(items.filter({ hasText: '随记' })).toHaveCount(0);
    await expect(items.filter({ hasText: '收件箱' })).toHaveCount(1);
  });

  test('offers a clipboard link once and stays quiet after 忽略', async ({ page }) => {
    await page.addInitScript(() => {
      const state = window as unknown as { __clipboardReads: number };
      state.__clipboardReads = 0;
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          readText: async () => {
            state.__clipboardReads += 1;
            return 'https://example.org/from-clipboard';
          },
        },
      });
    });
    await openReading(page);
    const banner = page.locator('.reading-clipboard');
    await expect(banner).toContainText('example.org');
    // 忽略后同一条内容不再打扰。
    await banner.getByRole('button', { name: '忽略' }).click();
    await expect(banner).toHaveCount(0);
  });

  test('does not touch the clipboard when the setting is off', async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.setItem('todo.app-settings.v1', JSON.stringify({ clipboardImport: false }));
      const state = window as unknown as { __clipboardReads: number };
      state.__clipboardReads = 0;
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          readText: async () => {
            state.__clipboardReads += 1;
            return 'https://example.org/from-clipboard';
          },
        },
      });
    });
    await openReading(page);
    await expect(page.locator('.reading-clipboard')).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __clipboardReads: number }).__clipboardReads)).toBe(0);
  });

  test('switching clipboard reading off in 设置 stops the 阅读 banner', async ({ page }) => {
    await page.addInitScript(() => {
      const state = window as unknown as { __clipboardReads: number };
      state.__clipboardReads = 0;
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          readText: async () => {
            state.__clipboardReads += 1;
            return 'https://example.org/from-clipboard';
          },
        },
      });
    });
    await page.goto('/');
    await page.getByRole('button', { name: '设置', exact: true }).click();
    const settings = page.getByRole('dialog', { name: '设置' });
    const toggle = settings.getByRole('switch', { name: '读取剪贴板识别链接' });
    await expect(toggle).toBeChecked();
    await toggle.click();
    await settings.getByRole('button', { name: '关闭设置' }).click();

    await nav(page).click();
    await expect(page.getByRole('heading', { name: '收件箱', level: 1 })).toBeVisible();
    await expect(page.locator('.reading-clipboard')).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __clipboardReads: number }).__clipboardReads)).toBe(0);
  });

  test('reopening restores the last position, offers 从头开始, and shows the marker', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    await expect(page.getByRole('heading', { name: '如何建立一个可持续的个人知识系统', level: 1 })).toBeVisible();
    const banner = page.getByRole('status').filter({ hasText: '已回到上次读到的位置 · 42%' });
    await expect(banner).toBeVisible();
    await expect(page.locator('.reader-body .reader-last-read')).toHaveCount(1);
    const pane = page.locator('.reader-pane');
    await expect.poll(async () => pane.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    await banner.getByRole('button', { name: '从头开始' }).click();
    await expect.poll(async () => pane.evaluate((element) => element.scrollTop)).toBe(0);
    await expect(banner).toHaveCount(0);
  });

  test('stored annotations revive and the note pane locates them', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    const note = page.getByTestId('reader-note');
    await expect(note).toHaveCount(1);
    await expect(note).toContainText('知识不是囤积，而是流动起来');
    await expect(page.getByRole('complementary', { name: '标记与笔记' })).toContainText('1 条标记 · 1 条笔记');
    await note.getByRole('button', { name: /知识的价值不在于拥有/ }).click();
    await expect(note).toHaveClass(/is-selected/);
  });

  test('selecting text creates a highlight, a note can be written, and the card can be removed', async ({ page }) => {
    await openReading(page);
    await page.getByTestId('reading-card').filter({ hasText: '深度工作' }).getByRole('button', { name: '开始阅读' }).click();
    await expect(page.getByTestId('reader-note')).toHaveCount(0);
    const paragraph = page.locator('.reader-body p').nth(1);
    const box = await paragraph.boundingBox();
    if (!box) throw new Error('paragraph not laid out');
    await page.mouse.move(box.x + 4, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 160, box.y + box.height / 2, { steps: 8 });
    await page.mouse.up();
    const selectionToolbar = page.getByRole('toolbar', { name: '标记操作' });
    await expect(selectionToolbar).toBeVisible();
    await expect(selectionToolbar.getByRole('button', { name: '标记并写笔记' })).toBeVisible();
    await selectionToolbar.getByRole('button', { name: '标记并写笔记' }).click();
    await expect(page.getByTestId('reader-note')).toHaveCount(1);
    await page.getByLabel('笔记内容').fill('这一段值得再读一遍');
    await page.getByRole('button', { name: '保存' }).click();
    await expect(page.getByTestId('reader-note')).toContainText('这一段值得再读一遍');
    await page.getByTestId('reader-note').getByRole('button', { name: '删除' }).click();
    await expect(page.getByTestId('reader-note')).toHaveCount(0);
  });

  test('scrolling to the end marks the article read and the list reflects it', async ({ page }) => {
    await openReading(page);
    await page.getByTestId('reading-card').filter({ hasText: '深度工作' }).getByRole('button', { name: '开始阅读' }).click();
    const pane = page.locator('.reader-pane');
    await pane.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
      element.dispatchEvent(new Event('scroll'));
    });
    await expect(page.locator('.reader__meta')).toContainText('已读 100%');
    await page.getByRole('button', { name: '返回列表' }).click();
    const card = page.getByTestId('reading-card').filter({ hasText: '深度工作' });
    await expect(card.locator('.reading-chip--read')).toHaveText(/已读/);
    await expect(nav(page)).toContainText('1');
  });

  test('标为已读 and 标为未读 round-trip from the reader bar', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    await page.getByRole('button', { name: '标为已读' }).click();
    await expect(page.locator('.reader__meta')).toContainText(/已读$/);
    await page.getByRole('button', { name: '标为未读' }).click();
    await expect(page.getByRole('button', { name: '标为已读' })).toBeVisible();
  });

  // 竖轨锚在 .reader__columns（不滚动）而非 .reader-pane（滚动容器）：
  // 放进 pane 会以滚动内容为包含块，随续读滚动滚出视口（zwmsdd 附带发现）。
  test('进度竖轨钉在视口内，滚动正文时不随内容滚出', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    const rail = page.locator('.reader-progress-rail');
    await expect(rail).toBeVisible();
    // .reader 的 view-in 入场动画（0.24s translateY, fill both）没结束前
    // boundingBox 带 y 偏移，等它结束再量。
    await page.locator('.reader').evaluate((element) => Promise.all(
      element.getAnimations({ subtree: true }).map((animation) => animation.finished.catch(() => {})),
    ));
    const pane = page.locator('.reader-pane');
    await pane.evaluate((element) => {
      element.scrollTop = element.scrollHeight / 2;
      element.dispatchEvent(new Event('scroll'));
    });
    const railBox = await rail.boundingBox();
    const paneBox = await pane.boundingBox();
    expect(railBox).not.toBeNull();
    expect(paneBox).not.toBeNull();
    // 视口锚定：滚到中部后轨道顶必须仍 = pane 顶 + 16px（旧实现锚在滚动
    // 内容上，此刻 y 会随内容上移甚至为负——单次测量即可钉死该回归）。
    expect(Math.abs(railBox!.y - (paneBox!.y + 16))).toBeLessThan(2);
    // 轨道全长 = pane 视口高 - 上下各 16px 内缩。
    expect(Math.abs(railBox!.height - (paneBox!.height - 32))).toBeLessThan(2);
    // 轨道贴正文右缘：right 与笔记栏宽度用同一 clamp 公式，两处必须逐像素同步。
    expect(Math.abs(railBox!.x + railBox!.width - (paneBox!.x + paneBox!.width - 3))).toBeLessThan(2);
  });

  test('正文右键不再提供另存为，改为阻止系统菜单（另存为收敛到列表卡右键）', async ({ page }) => {
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    const body = page.getByTestId('reader-body');
    await expect(body).toBeVisible();
    await body.click({ button: 'right' });
    // A′ 验收反馈：长按/右键不再弹另存为菜单，选区标注由标记工具条承担。
    await expect(page.getByRole('menu', { name: '另存文章' })).toHaveCount(0);
    // rctx 工具行的另存为入口也已删除。
    await expect(page.getByRole('button', { name: /另存为/ })).toHaveCount(0);
  });

  test('列表卡片右键同样弹出另存为菜单，不必先进文章', async ({ page }) => {
    await openReading(page);
    const card = page.getByTestId('reading-card').filter({ hasText: '深度工作' });
    await card.click({ button: 'right' });
    const menu = page.getByRole('menu', { name: '另存文章' });
    await expect(menu).toBeVisible();
    await expect(menu.getByRole('menuitem', { name: '另存为 Markdown' })).toBeVisible();
    await page.getByRole('menuitem', { name: '另存为 Markdown' }).click();
    await expect(menu).toHaveCount(0);
    await expect(page.getByTestId('reading-export-notice')).toContainText('浏览器版不支持另存到本机目录，请使用桌面端');
    // 换一篇文章右键，菜单跟着新文章走（提示条仍在时也可再次导出）。
    await card.click({ button: 'right' });
    await page.getByRole('menuitem', { name: '另存为 HTML' }).click();
    await expect(page.getByTestId('reading-export-notice')).toContainText('浏览器版不支持另存到本机目录，请使用桌面端');
  });

  // ≤1000px 时笔记不再是右栏，而是从底部拉出的抽屉；此前它在 auto 行里按
  // 40% 取高，只量到 73px，卡被裁掉也滚不到。
  test('narrow widths fold the notes into a bottom sheet', async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 800 });
    await openReading(page);
    await page.getByRole('button', { name: '继续阅读 · 42%' }).click();
    const notes = page.getByRole('complementary', { name: '标记与笔记' });
    const toggle = page.getByRole('button', { name: /标记与笔记，1 条标记 · 1 条笔记/ });
    await expect(toggle).toBeVisible();
    await expect(notes).toBeHidden();
    // 正文拿到全部高度，而不是被笔记挤成两半。
    const paneHeight = await page.locator('.reader-pane').evaluate((element) => element.getBoundingClientRect().height);
    expect(paneHeight).toBeGreaterThan(600);

    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(notes).toBeVisible();
    await expect(page.getByTestId('reader-note')).toHaveCount(1);
    // 卡片整个落在抽屉里，而不是被裁成一条细边。
    const card = await page.getByTestId('reader-note').first().boundingBox();
    const sheet = await notes.boundingBox();
    expect(card).not.toBeNull();
    expect(sheet).not.toBeNull();
    expect(card!.height).toBeGreaterThan(80);
    expect(card!.y + card!.height).toBeLessThanOrEqual(sheet!.y + sheet!.height + 1);

    await page.keyboard.press('Escape');
    await expect(notes).toBeHidden();

    await toggle.click();
    await page.getByRole('button', { name: '收起标记与笔记' }).click();
    await expect(notes).toBeHidden();
  });
});

test.describe('阅读上下文', () => {
  async function openFirstArticle(page: Page) {
    await openReading(page);
    await page.getByTestId('reading-card').first().getByRole('button', { name: /继续阅读|开始阅读|重读/ }).click();
    await expect(page.getByTestId('reading-context')).toBeVisible();
  }

  test('saves context, links project/task, and refetch keeps everything', async ({ page }) => {
    await openFirstArticle(page);
    const panel = page.getByTestId('reading-context');
    await page.getByRole('button', { name: '阅读上下文' }).click();

    await page.getByPlaceholder(/为什么保存这篇文章/).fill('研究 E2E 定位方式');
    await page.getByPlaceholder(/以后阅读时重点回答什么问题/).fill('能否兼容 Playwright？');
    await page.getByPlaceholder(/阅读后的理解与结论/).fill('可访问性树方案可行');
    // Blur the last field triggers one save.
    await page.getByPlaceholder(/阅读后的理解与结论/).blur();
    await expect(page.getByText('上下文已保存')).toBeVisible();

    // Link a project and a task by reference ID.
    await page.getByPlaceholder(/输入项目或任务 ID 关联/).fill('proj-1');
    await page.getByRole('button', { name: '关联项目' }).click();
    await expect(panel.locator('.rctx__chip', { hasText: 'proj-1' })).toBeVisible();
    await page.getByPlaceholder(/输入项目或任务 ID 关联/).fill('a1b2c3');
    await page.getByRole('button', { name: '关联任务' }).click();
    await expect(panel.locator('.rctx__chip', { hasText: 'a1b2c3' })).toBeVisible();

    // Re-parse: source refreshes, context/notes/links survive.
    await page.getByRole('button', { name: '重新解析' }).click();
    await expect(page.getByText('已重新解析（上下文与笔记保留）')).toBeVisible();
    await expect(panel.getByPlaceholder(/为什么保存这篇文章/)).toHaveValue('研究 E2E 定位方式');
    await expect(panel.getByPlaceholder(/阅读后的理解与结论/)).toHaveValue('可访问性树方案可行');
    await expect(panel.locator('.rctx__chip', { hasText: 'proj-1' })).toBeVisible();
    await expect(panel.locator('.rctx__chip', { hasText: 'a1b2c3' })).toBeVisible();

    // Export copies a Markdown reference card to the clipboard.
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.getByRole('button', { name: '导出引用' }).click();
    await expect(page.getByText('引用卡片已复制到剪贴板')).toBeVisible();
    const clipboard = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboard).toContain('# 收藏原因');
    expect(clipboard).toContain('研究 E2E 定位方式');
  });
});
