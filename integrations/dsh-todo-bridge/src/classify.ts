/**
 * Host-side replica of the digest panel's classification rules
 * (`packages/client/ui-digest/src/client/select.ts`: `eligible` + `classify`)
 * and the mapping from panel category to the contract `state` (ai-feed-spec
 * §2). Pure: no host imports, no clock reads.
 * @module dsh-todo-bridge/classify
 */

import type { BridgeCategory, BridgeDigest, BridgeInboxRow, BridgeSessionMeta, FeedState } from './types.ts'

/** Category plus the unread bit, as `select.ts` computes them. */
export interface Classification {
  readonly category: BridgeCategory
  readonly unread: boolean
}

/**
 * Whether the session may appear at all. `select.ts` excludes blank and
 * subagent-origin rows; the panel's caller additionally drops archived ids
 * (`ui-digest/src/client/index.ts`, badge recount), so archived is folded
 * in here.
 * @param meta - session facts.
 * @returns whether the row describes user-facing work.
 */
export function eligible(meta: BridgeSessionMeta): boolean {
  return !meta.blank && !meta.subagent && !meta.archived
}

/**
 * The seq of the newest thing that landed: the reply when there is one, else
 * the question (`select.ts` `landedSeq`).
 * @param digest - the digest value.
 * @returns the seq, or `null` before the first question.
 */
export function landedSeq(digest: BridgeDigest): number | null {
  return digest.replySeq ?? digest.questionSeq
}

/**
 * Classify one session exactly as the digest panel does.
 * @param digest - its digest value, possibly absent (no projection yet).
 * @param row - the user's durable marks, possibly absent.
 * @param meta - running / waiting / eligibility facts.
 * @param now - current time, for the snooze bound.
 * @returns the category and unread bit, or `null` when the row has nothing to report.
 */
export function classifyCategory(
  digest: BridgeDigest | undefined,
  row: BridgeInboxRow | undefined,
  meta: BridgeSessionMeta,
  now: number,
): Classification | null {
  if (!eligible(meta)) return null
  if (meta.waiting) return { category: 'needsYou', unread: false }
  if (meta.running) return { category: 'running', unread: false }
  if (digest?.question == null) return null
  const seenSeq = row?.lastSeenSeq ?? -1
  const landed = landedSeq(digest)
  const unread = landed !== null && landed > seenSeq
  if (row !== undefined && row.snoozedUntil !== null && row.snoozedUntil > now) {
    return { category: 'snoozed', unread }
  }
  if (row?.handledAt != null) return { category: 'handled', unread }
  if (digest.outcome !== 'completed') return { category: 'failed', unread }
  return { category: unread ? 'unread' : 'seen', unread }
}

/**
 * Contract mapping (ai-feed-spec §2): `snoozed` is not published.
 * @param category - a panel category.
 * @returns the contract state, or `null` when the category is not published.
 */
export function toFeedState(category: BridgeCategory): FeedState | null {
  switch (category) {
    case 'running': return 'running'
    case 'needsYou': return 'waiting'
    case 'unread': return 'unread'
    case 'seen': return 'seen'
    case 'failed': return 'failed'
    case 'handled': return 'handled'
    case 'snoozed': return null
  }
}

/**
 * The bridge's one classification entry: panel rules, then the contract
 * mapping. `null` means "do not publish" (ineligible, nothing to report, or
 * snoozed); the bridge never retracts a record it published earlier.
 * @param digest - digest value, possibly absent.
 * @param row - inbox marks, possibly absent.
 * @param meta - session facts.
 * @param now - current time.
 * @returns the contract state or `null`.
 */
export function classify(
  digest: BridgeDigest | undefined,
  row: BridgeInboxRow | undefined,
  meta: BridgeSessionMeta,
  now: number,
): FeedState | null {
  const classified = classifyCategory(digest, row, meta, now)
  return classified === null ? null : toFeedState(classified.category)
}
