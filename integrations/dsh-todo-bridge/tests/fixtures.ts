/**
 * Fixtures mirroring the digest panel's `select.ts` cases: one digest per
 * turn shape, one inbox row per user mark, one meta per runtime situation.
 */
import type { BridgeDigest, BridgeInboxRow, BridgeSessionMeta } from '../src/types.ts'

export const NOW = 1_757_550_000_000

export const WORKSPACE = '/Users/lens/lens077/ToDo'

export function digest(overrides: Partial<BridgeDigest> = {}): BridgeDigest {
  return {
    question: '补一段 cargo install 的说明',
    questionTruncated: false,
    questionSeq: 10,
    questionAt: NOW - 60_000,
    reply: '已在 README 快速开始加入 cargo install --path …',
    replyTruncated: false,
    outcome: 'completed',
    replySeq: 14,
    repliedAt: NOW - 30_000,
    changedFiles: [`${WORKSPACE}/README.md`],
    changedFileCount: 1,
    ...overrides,
  }
}

/** A question whose turn is still open: no reply, no outcome. */
export function openDigest(overrides: Partial<BridgeDigest> = {}): BridgeDigest {
  return digest({ reply: null, replySeq: null, repliedAt: null, outcome: null, changedFiles: [], changedFileCount: 0, ...overrides })
}

export function row(overrides: Partial<BridgeInboxRow> = {}): BridgeInboxRow {
  return {
    lastSeenSeq: null,
    handledAt: null,
    snoozedUntil: null,
    pinned: false,
    updatedAt: NOW - 10_000,
    ...overrides,
  }
}

export function meta(overrides: Partial<BridgeSessionMeta> = {}): BridgeSessionMeta {
  return {
    sessionId: 's_8f2a',
    workspace: WORKSPACE,
    title: '文档｜cargo install 说明',
    createdAt: NOW - 120_000,
    running: false,
    waiting: false,
    blank: false,
    subagent: false,
    archived: false,
    signalAt: null,
    ...overrides,
  }
}
