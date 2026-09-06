import { expect, test, type Page } from '@playwright/test';
import type { DesktopMutation, DesktopSnapshot } from '../src/lib/desktop';

const taskId = 'fresh01';

function snapshot(revision: string, priority: 1 | 2): DesktopSnapshot {
  const task = {
    id: taskId,
    body: 'Snapshot freshness fixture',
    completed: false,
    column: 'todo',
    indent_level: 0,
    parent_id: null,
    due: null,
    priority,
    tags: [],
    progress: null,
    failed_on: null,
    note: null,
    evidence: null,
    evidence_availability: { status: 'unavailable' as const, reason: 'Fixture has no Git history.' },
  };
  return {
    project_path: '/fixtures/snapshot-freshness/TODO.md',
    revision,
    completion_policy: 'normal',
    columns: [{ id: 'todo', name: 'Todo', done: false }],
    tasks: [task],
    today: { overdue: [], today: [], upcoming: [], next: [task] },
  };
}

const stateA = snapshot('aaaaaaaaaaaaaaaa', 2);
const stateB = snapshot('bbbbbbbbbbbbbbbb', 1);

interface FreshnessHarness {
  setDisk(value: DesktopSnapshot): void;
  emitSnapshot(value: DesktopSnapshot): void;
  holdNextRead(): number;
  pendingReads(): number[];
  releaseRead(id: number, error?: string): void;
  stats(): { reads: number; mutations: number; revision: string; snapshotListeners: number };
}

declare global {
  interface Window {
    __snapshotFreshness: FreshnessHarness;
  }
}

async function installNativeBridge(
  page: Page,
  states = [stateA, stateB],
  holdInitialRead = false,
): Promise<void> {
  // Mirror Tauri's mocks.js IPC/callback shape while retaining the real bridge
  // adapter and hook. A held read captures disk at invocation, not at release.
  await page.addInitScript(({ states, holdInitialRead }) => {
    let disk = structuredClone(states[0]);
    let reads = 0;
    let mutations = 0;
    let callbackSequence = 0;
    let holdSequence = holdInitialRead ? 1 : 0;
    const queuedHolds: number[] = holdInitialRead ? [0] : [];
    const callbacks = new Map<number, (value: unknown) => void>();
    const listeners = new Map<string, Set<number>>();
    const pending = new Map<number, {
      value: DesktopSnapshot;
      resolve: (value: DesktopSnapshot) => void;
      reject: (error: string) => void;
    }>();

    function unregisterCallback(id: number): void {
      callbacks.delete(id);
    }

    function unregisterListener(event: string, id: number): void {
      listeners.get(event)?.delete(id);
      unregisterCallback(id);
    }

    function runCallback(id: number, value: unknown): void {
      callbacks.get(id)?.(value);
    }

    function emit(event: string, payload: unknown): void {
      for (const id of listeners.get(event) ?? []) {
        runCallback(id, { event, id, payload: structuredClone(payload) });
      }
    }

    Object.assign(window, {
      __TAURI_INTERNALS__: {
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { windowLabel: 'main', label: 'main' },
        },
        callbacks,
        transformCallback(callback: (value: unknown) => void, once = false) {
          const id = ++callbackSequence;
          callbacks.set(id, (value) => {
            if (once) unregisterCallback(id);
            callback(value);
          });
          return id;
        },
        unregisterCallback,
        runCallback,
        async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
          switch (command) {
            case 'desktop_status':
              return { startup_ms: 1, shortcut_status: { status: 'registered' } };
            case 'capture_list':
              return [];
            case 'snapshot': {
              reads += 1;
              const value = structuredClone(disk);
              const hold = queuedHolds.shift();
              if (hold === undefined) return value;
              return new Promise<DesktopSnapshot>((resolve, reject) => {
                pending.set(hold, { value, resolve, reject });
              });
            }
            case 'mutate': {
              const intent = args.intent as DesktopMutation;
              if (intent?.type !== 'set_priority' || intent.id !== 'fresh01') {
                throw new Error('Unexpected mutation in snapshot freshness fixture');
              }
              const next = states.find((value) => value.tasks[0].priority === intent.priority);
              if (!next) throw new Error(`Unconfigured fixture priority: ${intent.priority}`);
              mutations += 1;
              disk = structuredClone(next);
              return structuredClone(disk);
            }
            case 'plugin:event|listen': {
              const event = String(args.event);
              const id = Number(args.handler);
              const handlers = listeners.get(event) ?? new Set<number>();
              handlers.add(id);
              listeners.set(event, handlers);
              return id;
            }
            case 'plugin:event|unlisten':
              unregisterListener(String(args.event), Number(args.eventId ?? args.id));
              return null;
            case 'plugin:event|emit':
              // Also harmlessly accepts the temporary native drag diagnostics.
              emit(String(args.event), args.payload);
              return null;
            default:
              throw new Error(`Unexpected Tauri command: ${command}`);
          }
        },
      },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener },
    });

    window.__snapshotFreshness = {
      setDisk(value) { disk = structuredClone(value); },
      emitSnapshot(value) { emit('desktop://snapshot-changed', value); },
      holdNextRead() {
        const id = holdSequence++;
        queuedHolds.push(id);
        return id;
      },
      pendingReads() { return Array.from(pending.keys()); },
      releaseRead(id, error) {
        const read = pending.get(id);
        if (!read) throw new Error(`Snapshot read ${id} has not started or already settled`);
        pending.delete(id);
        if (error === undefined) read.resolve(read.value);
        else read.reject(error);
      },
      stats() {
        return {
          reads,
          mutations,
          revision: disk.revision,
          snapshotListeners: listeners.get('desktop://snapshot-changed')?.size ?? 0,
        };
      },
    };
  }, { states, holdInitialRead });
}

async function showQuadrant(page: Page): Promise<void> {
  await page.getByRole('button', { name: '\u8c61\u9650', exact: true }).click();
}

async function expectSnapshot(page: Page, value: DesktopSnapshot): Promise<void> {
  const priority = value.tasks[0].priority;
  await expect(page.locator(`section[aria-labelledby="quadrant-${priority}"] [data-task-id="${taskId}"]`)).toBeVisible();
  await expect(page.locator('.revision-chip')).toHaveAttribute('title', `Snapshot revision ${value.revision}`);
  await expect(page.locator('.status-banner')).toHaveCount(0);
}

async function openFixture(page: Page, states = [stateA, stateB]): Promise<void> {
  await installNativeBridge(page, states);
  await page.goto('/');
  await showQuadrant(page);
  await expectSnapshot(page, states[0]);
  await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.stats().snapshotListeners)).toBeGreaterThan(0);
}

async function mutatePriority(page: Page, value: DesktopSnapshot): Promise<void> {
  const before = await page.evaluate(() => window.__snapshotFreshness.stats().mutations);
  const card = page.locator(`[data-task-id="${taskId}"]`);
  await expect(card).not.toHaveClass(/is-busy/);
  await card.press(String(value.tasks[0].priority));
  await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.stats().mutations)).toBe(before + 1);
  await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.stats().revision)).toBe(value.revision);
  await expectSnapshot(page, value);
}

async function holdRefresh(page: Page): Promise<number> {
  const id = await page.evaluate(() => window.__snapshotFreshness.holdNextRead());
  await page.locator('.command-refresh').click();
  await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.pendingReads())).toContain(id);
  return id;
}

async function flushUi(page: Page): Promise<void> {
  // The response/event is explicitly released first; two frame boundaries let
  // its promise chain and React commit drain before asserting no regression.
  await page.evaluate(() => new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  }));
}

async function releaseRead(page: Page, id: number, error?: string): Promise<void> {
  await page.evaluate(({ id, error }) => window.__snapshotFreshness.releaseRead(id, error), { id, error });
  await flushUi(page);
}

test.describe('native snapshot freshness', () => {
  test('authoritative priority mutations can return to the same content hash repeatedly', async ({ page }) => {
    await openFixture(page);
    for (const value of [stateB, stateA, stateB, stateA]) await mutatePriority(page, value);
  });

  test('accepts a numerically lower 16-character content hash', async ({ page }) => {
    const high = snapshot('9999999999999999', 2);
    const low = snapshot('1111111111111111', 1);
    await openFixture(page, [high, low]);
    await mutatePriority(page, low);
    await mutatePriority(page, high);
  });

  test('a delayed watcher payload cannot undo the current authoritative snapshot', async ({ page }) => {
    await openFixture(page);
    await mutatePriority(page, stateB);
    await mutatePriority(page, stateA);
    const before = await page.evaluate(() => window.__snapshotFreshness.stats().reads);
    await page.evaluate((old) => window.__snapshotFreshness.emitSnapshot(old), stateB);
    await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.stats().reads)).toBeGreaterThan(before);
    await flushUi(page);
    await expectSnapshot(page, stateA);
  });

  test('external edits may legitimately revisit an earlier content hash', async ({ page }) => {
    await openFixture(page);
    for (const value of [stateB, stateA]) {
      const before = await page.evaluate(() => window.__snapshotFreshness.stats().reads);
      await page.evaluate((next) => {
        window.__snapshotFreshness.setDisk(next);
        window.__snapshotFreshness.emitSnapshot(next);
      }, value);
      await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.stats().reads)).toBeGreaterThan(before);
      await expectSnapshot(page, value);
    }
    expect(await page.evaluate(() => window.__snapshotFreshness.stats().mutations)).toBe(0);
  });

  test('a snapshot read captured before a successful mutation cannot roll it back', async ({ page }) => {
    await openFixture(page);
    const oldRead = await holdRefresh(page);
    await mutatePriority(page, stateB);
    await releaseRead(page, oldRead);
    await expectSnapshot(page, stateB);
  });

  test('an authoritative no-op still invalidates an older in-flight read', async ({ page }) => {
    await openFixture(page);
    await page.evaluate((next) => window.__snapshotFreshness.setDisk(next), stateB);
    const oldRead = await holdRefresh(page);
    await page.evaluate((next) => window.__snapshotFreshness.setDisk(next), stateA);
    await mutatePriority(page, stateA);
    await releaseRead(page, oldRead);
    await expectSnapshot(page, stateA);
  });

  test('a late startup read cannot replace a watcher load followed by a mutation', async ({ page }) => {
    await installNativeBridge(page, [stateA, stateB], true);
    await page.goto('/');
    await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.pendingReads())).toContain(0);
    await expect.poll(() => page.evaluate(() => window.__snapshotFreshness.stats().snapshotListeners)).toBeGreaterThan(0);
    await page.evaluate((value) => window.__snapshotFreshness.emitSnapshot(value), stateA);
    await showQuadrant(page);
    await expectSnapshot(page, stateA);
    await mutatePriority(page, stateB);
    await releaseRead(page, 0);
    await expectSnapshot(page, stateB);
  });

  for (const outcome of ['success', 'error'] as const) {
    test(`a late refresh ${outcome} cannot replace a newer successful refresh`, async ({ page }) => {
      await openFixture(page);
      const older = await holdRefresh(page);
      await page.evaluate((value) => window.__snapshotFreshness.setDisk(value), stateB);
      const newer = await holdRefresh(page);
      await releaseRead(page, newer);
      await expectSnapshot(page, stateB);
      await releaseRead(page, older, outcome === 'error' ? 'frontmatter parse failed in stale read' : undefined);
      await expectSnapshot(page, stateB);
      await expect(page.getByRole('alert')).toHaveCount(0);
    });
  }
});
