/**
 * Bridge-local input and output vocabulary. The pure modules (`classify`,
 * `record`, `publisher`) depend only on these shapes, so they are testable
 * without the DeepSeek Harness type chain; `index.ts` maps the host's real
 * services onto them.
 * @module dsh-todo-bridge/types
 */

/** Terminal reason of a turn, mirroring `SessionDigestOutcome` of `@deepseek-ai/dsh-session-digest`. */
export type BridgeOutcome = 'completed' | 'error' | 'aborted' | 'blocked' | 'max-tokens' | 'interrupted'

/** The subset of the host `sessionDigest` projection value the bridge reads. */
export interface BridgeDigest {
  /** Newest human question; `null` before the first one. */
  readonly question: string | null
  readonly questionTruncated: boolean
  /** Log seq of the newest question's `user/message`. */
  readonly questionSeq: number | null
  /** Event time of the newest question, epoch ms. */
  readonly questionAt: number | null
  /** Closing assistant message of the turn answering `question`. */
  readonly reply: string | null
  readonly replyTruncated: boolean
  /** Terminal reason of the current question's turn; `null` while open. */
  readonly outcome: BridgeOutcome | null
  /** Log seq of the retained assistant message. */
  readonly replySeq: number | null
  /** Event time of the retained assistant message, epoch ms. */
  readonly repliedAt: number | null
  /** Distinct file paths the current turn changed, as the host recorded them. */
  readonly changedFiles: readonly string[]
  readonly changedFileCount: number
}

/** The user's durable inbox marks on one session (`InboxSessionState` of `@deepseek-ai/dsh-session-inbox`). */
export interface BridgeInboxRow {
  /** Highest log seq the user had on screen; `null` when never opened through the inbox-aware client. */
  readonly lastSeenSeq: number | null
  /** When the user marked the outcome as dealt with; `null` when open. */
  readonly handledAt: number | null
  /** Hidden until this epoch-ms time; `null` when not snoozed. */
  readonly snoozedUntil: number | null
  readonly pinned: boolean
  /** Host-assigned time of the most recent change to the row. */
  readonly updatedAt: number
}

/** Session facts outside the digest and the inbox row. */
export interface BridgeSessionMeta {
  readonly sessionId: string
  /** Absolute working directory of the session; `null` when the header carries none. */
  readonly workspace: string | null
  /** Current session title; `null` before one lands. */
  readonly title: string | null
  /** Session creation time, epoch ms. */
  readonly createdAt: number
  /** The agent is driving a turn right now. */
  readonly running: boolean
  /** An approval or user question is pending on this session. */
  readonly waiting: boolean
  /** No `turn/start` has happened yet. */
  readonly blank: boolean
  /** Created as a subagent child. */
  readonly subagent: boolean
  /** In the workspace registry's archive set. */
  readonly archived: boolean
  /**
   * Time of the newest host-side runtime signal for this session (agent status
   * flip, pending interaction start or end, inbox row change, archive toggle),
   * epoch ms; `null` when none was observed in this plugin life.
   */
  readonly signalAt: number | null
}

/** Digest-panel category, mirroring `InboxCategory` of `@deepseek-ai/dsh-client-ui-digest`. */
export type BridgeCategory = 'unread' | 'seen' | 'running' | 'needsYou' | 'failed' | 'handled' | 'snoozed'

/** Contract `state` values (ai-feed-spec §2). */
export type FeedState = 'running' | 'waiting' | 'unread' | 'seen' | 'failed' | 'handled'

/** One ai-feed-spec 1 record as the host writes it (§1.1). */
export interface FeedRecord {
  readonly 'ai-feed-spec': 1
  readonly source: 'dsh'
  readonly session_id: string
  readonly workspace: string
  readonly state: FeedState
  readonly outcome: BridgeOutcome | null
  readonly title: string | null
  readonly question: string | null
  readonly reply: string | null
  readonly reply_truncated: boolean
  readonly changed_files: readonly string[]
  readonly changed_file_count: number
  readonly open_url: string | null
  readonly event_at: number
}
