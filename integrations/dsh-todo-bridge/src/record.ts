/**
 * Pure builder of one ai-feed-spec 1 record (§1.1) from the bridge inputs:
 * byte-bounded text fields, a bounded relative changed-file list, and
 * `event_at` taken from source-side event times, never from the clock at
 * send time (§6).
 * @module dsh-todo-bridge/record
 */

import { Buffer } from 'node:buffer'
import { isAbsolute, relative, sep } from 'node:path'
import type { BridgeDigest, BridgeInboxRow, BridgeSessionMeta, FeedRecord, FeedState } from './types.ts'

/** Contract byte limits (ai-feed-spec §1.1). */
export const CONTRACT_LIMITS = Object.freeze({
  sessionIdBytes: 128,
  titleBytes: 200,
  questionBytes: 2000,
  replyBytes: 4000,
  changedFiles: 32,
})

/** Result of a byte-bounded truncation. */
export interface Truncated {
  readonly text: string
  readonly truncated: boolean
}

/**
 * Cut `text` to at most `maxBytes` UTF-8 bytes on a code-point boundary.
 * @param text - source text.
 * @param maxBytes - inclusive byte cap.
 * @returns the text or its longest allowed prefix, and whether it was cut.
 */
export function truncateUtf8(text: string, maxBytes: number): Truncated {
  if (Buffer.byteLength(text, 'utf8') <= maxBytes) return { text, truncated: false }
  let bytes = 0
  let end = 0
  for (const codePoint of text) {
    const size = Buffer.byteLength(codePoint, 'utf8')
    if (bytes + size > maxBytes) break
    bytes += size
    end += codePoint.length
  }
  return { text: text.slice(0, end), truncated: true }
}

/**
 * Express one changed-file path relative to the workspace when it lies
 * inside it; any other path is passed through as the host recorded it.
 * @param file - path as the mutation tool recorded it.
 * @param workspace - absolute session workspace.
 * @returns a workspace-relative path when possible.
 */
export function relativeToWorkspace(file: string, workspace: string): string {
  if (!isAbsolute(file)) return file
  const rel = relative(workspace, file)
  if (rel === '' || rel.startsWith('..') || isAbsolute(rel)) return file
  return sep === '/' ? rel : rel.split(sep).join('/')
}

/** Inputs of {@link buildRecord}. */
export interface BuildRecordInput {
  readonly meta: BridgeSessionMeta
  readonly digest: BridgeDigest | undefined
  readonly row: BridgeInboxRow | undefined
  readonly state: FeedState
  /** Link opening the session, or `null` when the host has none. */
  readonly openUrl: string | null
}

/**
 * Source-side time of the state the record describes: the newest of the
 * digest's question/reply times, the inbox row's change time, the newest
 * runtime signal, and the session creation time as the floor.
 * @param input - the same inputs as the record.
 * @returns epoch ms.
 */
export function eventAtOf(input: Pick<BuildRecordInput, 'meta' | 'digest' | 'row'>): number {
  const candidates = [
    input.meta.createdAt,
    input.digest?.questionAt ?? 0,
    input.digest?.repliedAt ?? 0,
    input.row?.updatedAt ?? 0,
    input.meta.signalAt ?? 0,
  ]
  return Math.max(...candidates)
}

/**
 * Build one contract record. The workspace must be known (the contract
 * requires it); the caller filters sessions without one.
 * @param input - session facts, digest, inbox row, the classified state, and the open link.
 * @returns the JSON object to hand to `todo ai upsert --stdin`.
 * @throws when the session has no workspace.
 */
export function buildRecord(input: BuildRecordInput): FeedRecord {
  const { meta, digest, state } = input
  if (meta.workspace === null) {
    throw new Error(`todo-bridge: session ${meta.sessionId} has no workspace; the contract requires one`)
  }
  const openTurn = state === 'running' || state === 'waiting'
  const question = digest?.question ?? null
  const reply = digest?.reply ?? null
  const cutReply = reply === null ? null : truncateUtf8(reply, CONTRACT_LIMITS.replyBytes)
  const files = (digest?.changedFiles ?? [])
    .map(file => relativeToWorkspace(file, meta.workspace as string))
    .slice(0, CONTRACT_LIMITS.changedFiles)
  const changedFileCount = Math.max(digest?.changedFileCount ?? 0, digest?.changedFiles.length ?? 0)
  return {
    'ai-feed-spec': 1,
    source: 'dsh',
    session_id: truncateUtf8(meta.sessionId, CONTRACT_LIMITS.sessionIdBytes).text,
    workspace: meta.workspace,
    state,
    // A turn that is still open has no outcome yet (§2: running → outcome null).
    outcome: openTurn ? null : (digest?.outcome ?? null),
    title: meta.title === null ? null : truncateUtf8(meta.title, CONTRACT_LIMITS.titleBytes).text,
    question: question === null ? null : truncateUtf8(question, CONTRACT_LIMITS.questionBytes).text,
    reply: cutReply === null ? null : cutReply.text,
    reply_truncated: (digest?.replyTruncated ?? false) || (cutReply?.truncated ?? false),
    changed_files: files,
    changed_file_count: changedFileCount,
    open_url: input.openUrl,
    event_at: eventAtOf(input),
  }
}

/**
 * Stable identity of a record's published content, ignoring `event_at`, so
 * a re-evaluation that changes nothing visible does not advance ToDo's
 * `event_at` (which would clear the user's local `acked_at`).
 * @param record - a built record.
 * @returns a comparison key.
 */
export function contentKey(record: FeedRecord): string {
  const { event_at: _ignored, ...rest } = record
  return JSON.stringify(rest)
}
