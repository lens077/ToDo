/**
 * 一次性引导提示的旗标（docs/onboarding-spec.md §5.1）。
 *
 * 状态只存本机 localStorage，不写进用户仓库；读写失败时按「没看过」处理，
 * 最多再提示一次，不会阻断任何操作。
 */
const RECONCILE_REVEALED_KEY = 'todo.onboarding.reconcile-revealed.v1';
const AGENT_HINT_KEY = 'todo.onboarding.agent-hint.v1';

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

function readFlag(key: string): boolean {
  try {
    return storage()?.getItem(key) === '1';
  } catch {
    return false;
  }
}

function writeFlag(key: string): void {
  try {
    storage()?.setItem(key, '1');
  } catch {
    // 存储不可用时本次提示仍可关闭，只是下次启动会再出现一次。
  }
}

export function wasReconcileRevealed(): boolean {
  return readFlag(RECONCILE_REVEALED_KEY);
}

export function markReconcileRevealed(): void {
  writeFlag(RECONCILE_REVEALED_KEY);
}

export function wasAgentHintDismissed(): boolean {
  return readFlag(AGENT_HINT_KEY);
}

export function markAgentHintDismissed(): void {
  writeFlag(AGENT_HINT_KEY);
}
