import { describe, expect, it } from 'vitest'
import { classify, classifyCategory, eligible, landedSeq, toFeedState } from '../src/classify.ts'
import type { BridgeCategory } from '../src/types.ts'
import { NOW, digest, meta, openDigest, row } from './fixtures.ts'

describe('eligible (select.ts eligible + archived filter)', () => {
  it('admits an ordinary session', () => {
    expect(eligible(meta())).toBe(true)
  })
  it('drops blank, subagent-origin, and archived sessions', () => {
    expect(eligible(meta({ blank: true }))).toBe(false)
    expect(eligible(meta({ subagent: true }))).toBe(false)
    expect(eligible(meta({ archived: true }))).toBe(false)
  })
})

describe('landedSeq', () => {
  it('prefers the reply seq and falls back to the question seq', () => {
    expect(landedSeq(digest())).toBe(14)
    expect(landedSeq(openDigest())).toBe(10)
    expect(landedSeq(openDigest({ questionSeq: null }))).toBeNull()
  })
})

describe('classifyCategory (select.ts classify)', () => {
  it('waiting wins over everything, even a running agent', () => {
    expect(classifyCategory(digest(), row({ handledAt: NOW }), meta({ waiting: true, running: true }), NOW))
      .toEqual({ category: 'needsYou', unread: false })
  })

  it('running wins over the digest and the marks', () => {
    expect(classifyCategory(digest({ outcome: 'error' }), row({ handledAt: NOW }), meta({ running: true }), NOW))
      .toEqual({ category: 'running', unread: false })
  })

  it('reports nothing for a session with no question yet', () => {
    expect(classifyCategory(undefined, undefined, meta(), NOW)).toBeNull()
    expect(classifyCategory(digest({ question: null }), undefined, meta(), NOW)).toBeNull()
  })

  it('is unread when the reply landed after the seen mark, or was never seen', () => {
    expect(classifyCategory(digest(), undefined, meta(), NOW)).toEqual({ category: 'unread', unread: true })
    expect(classifyCategory(digest(), row({ lastSeenSeq: 13 }), meta(), NOW)).toEqual({ category: 'unread', unread: true })
  })

  it('is seen once the seen mark covers the landed seq', () => {
    expect(classifyCategory(digest(), row({ lastSeenSeq: 14 }), meta(), NOW)).toEqual({ category: 'seen', unread: false })
    expect(classifyCategory(digest(), row({ lastSeenSeq: 99 }), meta(), NOW)).toEqual({ category: 'seen', unread: false })
  })

  it('uses the question seq as the landed seq when there is no reply', () => {
    expect(classifyCategory(openDigest({ outcome: 'completed' }), row({ lastSeenSeq: 9 }), meta(), NOW)?.category).toBe('unread')
    expect(classifyCategory(openDigest({ outcome: 'completed' }), row({ lastSeenSeq: 10 }), meta(), NOW)?.category).toBe('seen')
  })

  it('is snoozed while the snooze time is in the future, keeping the unread bit', () => {
    expect(classifyCategory(digest(), row({ snoozedUntil: NOW + 1 }), meta(), NOW)).toEqual({ category: 'snoozed', unread: true })
    expect(classifyCategory(digest(), row({ snoozedUntil: NOW + 1, lastSeenSeq: 14 }), meta(), NOW)).toEqual({ category: 'snoozed', unread: false })
  })

  it('treats an expired snooze as no snooze', () => {
    expect(classifyCategory(digest(), row({ snoozedUntil: NOW }), meta(), NOW)?.category).toBe('unread')
    expect(classifyCategory(digest(), row({ snoozedUntil: NOW - 1 }), meta(), NOW)?.category).toBe('unread')
  })

  it('handled beats failed and unread; snoozed beats handled', () => {
    expect(classifyCategory(digest({ outcome: 'error' }), row({ handledAt: NOW }), meta(), NOW)).toEqual({ category: 'handled', unread: true })
    expect(classifyCategory(digest({ outcome: 'error' }), row({ handledAt: NOW, snoozedUntil: NOW + 5 }), meta(), NOW)?.category).toBe('snoozed')
  })

  it('is failed for every non-completed outcome, including a still-open turn with no running agent', () => {
    for (const outcome of ['error', 'aborted', 'blocked', 'max-tokens', 'interrupted'] as const) {
      expect(classifyCategory(digest({ outcome }), undefined, meta(), NOW)?.category).toBe('failed')
    }
    expect(classifyCategory(openDigest(), undefined, meta(), NOW)).toEqual({ category: 'failed', unread: true })
  })

  it('never reports an ineligible session, whatever its signals', () => {
    expect(classifyCategory(digest(), undefined, meta({ blank: true, running: true }), NOW)).toBeNull()
    expect(classifyCategory(digest(), undefined, meta({ subagent: true, waiting: true }), NOW)).toBeNull()
    expect(classifyCategory(digest(), undefined, meta({ archived: true }), NOW)).toBeNull()
  })
})

describe('toFeedState (ai-feed-spec §2)', () => {
  it('maps every category; snoozed is not published', () => {
    const expected: Record<BridgeCategory, string | null> = {
      running: 'running',
      needsYou: 'waiting',
      unread: 'unread',
      seen: 'seen',
      failed: 'failed',
      handled: 'handled',
      snoozed: null,
    }
    for (const [category, state] of Object.entries(expected)) {
      expect(toFeedState(category as BridgeCategory)).toBe(state)
    }
  })
})

describe('classify (end to end)', () => {
  it('composes the panel rule and the contract mapping', () => {
    expect(classify(digest(), undefined, meta({ waiting: true }), NOW)).toBe('waiting')
    expect(classify(digest(), undefined, meta({ running: true }), NOW)).toBe('running')
    expect(classify(digest(), undefined, meta(), NOW)).toBe('unread')
    expect(classify(digest(), row({ lastSeenSeq: 14 }), meta(), NOW)).toBe('seen')
    expect(classify(digest({ outcome: 'aborted' }), undefined, meta(), NOW)).toBe('failed')
    expect(classify(digest(), row({ handledAt: NOW }), meta(), NOW)).toBe('handled')
    expect(classify(digest(), row({ snoozedUntil: NOW + 60_000 }), meta(), NOW)).toBeNull()
    expect(classify(undefined, undefined, meta(), NOW)).toBeNull()
  })
})
