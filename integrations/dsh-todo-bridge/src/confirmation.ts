/**
 * v2 confirmation objects (ai-feed-spec §1.1 `action` / `params`): what a
 * waiting session is asking the user to approve, and the parameters that
 * approval would authorize.
 *
 * ToDo computes `params_digest` from the exact `params` string, so this module
 * must be deterministic: the same pending interaction always serializes to the
 * same bytes, or an approval would silently invalidate itself.
 * @module dsh-todo-bridge/confirmation
 */

import { Buffer } from 'node:buffer'
import type { Confirmation } from './types.ts'

/** Contract byte caps for the confirmation fields (ai-feed-spec §1.1). */
export const CONFIRMATION_LIMITS = Object.freeze({
  actionBytes: 200,
  paramsBytes: 4096,
})

/** Ellipsis appended to a truncated human-readable label (UTF-8 bytes). */
const ELLIPSIS = '…'

/** The subset of a pending question a confirmation object carries. */
export interface ConfirmationQuestion {
  readonly id: string
  readonly question: string
  readonly header?: string | undefined
  readonly detail?: string | undefined
  readonly options?: readonly ConfirmationOption[] | undefined
  readonly multiSelect?: boolean | undefined
}

/** The subset of a question option the params object carries. */
export interface ConfirmationOption {
  readonly label: string
  readonly description?: string | undefined
}

/** The approval facts a pending tool call exposes. */
export interface ApprovalFacts {
  /** Tool whose operation needs the decision. */
  readonly toolName: string
  /** Exact tool call under review, when the host identified it. */
  readonly callId?: string | undefined
  /** Human-readable reason supplied by the asker. */
  readonly reason?: string | undefined
  /** The call's arguments as the host logged them (a JSON string). */
  readonly argumentsJson?: string | undefined
}

/** Cut text to a byte budget, marking the cut with an ellipsis when it fits. */
function fitText(text: string, maxBytes: number): string {
  if (Buffer.byteLength(text, 'utf8') <= maxBytes) return text
  const kept = maxBytes - Buffer.byteLength(ELLIPSIS, 'utf8')
  if (kept <= 0) return truncateToBytes(text, maxBytes)
  return `${truncateToBytes(text, kept)}${ELLIPSIS}`
}

/** Keep the longest code-point-aligned prefix within `maxBytes`. */
function truncateToBytes(text: string, maxBytes: number): string {
  let bytes = 0
  let end = 0
  for (const codePoint of text) {
    const size = Buffer.byteLength(codePoint, 'utf8')
    if (bytes + size > maxBytes) break
    bytes += size
    end += codePoint.length
  }
  return text.slice(0, end)
}

/**
 * Serialize a params object within the contract budget.
 *
 * `params` is what the user reviews before approving, so the full value is
 * preferred; when it exceeds the cap, string leaves and array lengths shrink in
 * passes, and a minimal object with a `paramsTruncated` marker is the last
 * resort. The result is always a JSON object, which the contract requires.
 */
function fitParams(value: Record<string, unknown>): string {
  const full = JSON.stringify(value)
  if (Buffer.byteLength(full, 'utf8') <= CONFIRMATION_LIMITS.paramsBytes) return full
  for (const leafBytes of [512, 128, 32]) {
    const shrunk = JSON.stringify(shrinkValue(value, leafBytes))
    if (Buffer.byteLength(shrunk, 'utf8') <= CONFIRMATION_LIMITS.paramsBytes) return shrunk
  }
  const minimal: Record<string, unknown> = {}
  for (const key of ['kind', 'tool', 'callId', 'reason'] as const) {
    const field = value[key]
    if (typeof field === 'string') minimal[key] = fitText(field, 64)
  }
  minimal.paramsTruncated = true
  const text = JSON.stringify(minimal)
  return Buffer.byteLength(text, 'utf8') <= CONFIRMATION_LIMITS.paramsBytes
    ? text
    : '{"paramsTruncated":true}'
}

/** Recursively cap string leaves and array lengths for one shrink pass. */
function shrinkValue(value: Record<string, unknown>, leafBytes: number): Record<string, unknown> {
  const shrink = (candidate: unknown): unknown => {
    if (typeof candidate === 'string') return fitText(candidate, leafBytes)
    if (Array.isArray(candidate)) return candidate.slice(0, 8).map(shrink)
    if (candidate !== null && typeof candidate === 'object') {
      const out: Record<string, unknown> = {}
      for (const [key, nested] of Object.entries(candidate)) out[key] = shrink(nested)
      return out
    }
    return candidate
  }
  return shrink(value) as Record<string, unknown>
}

/** Parse outer arguments and keep them only when the contract sees an object. */
function argumentsOf(argumentsJson: string | undefined): Record<string, unknown> | undefined {
  if (argumentsJson === undefined || argumentsJson.trim() === '') return undefined
  let parsed: unknown
  try {
    parsed = JSON.parse(argumentsJson)
  } catch {
    return undefined
  }
  return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
    ? parsed as Record<string, unknown>
    : undefined
}

/**
 * Build the confirmation object for a pending tool approval.
 * @param facts - tool identity and the arguments the host logged for it.
 * @returns the confirmation, or `null` when the tool cannot be named.
 */
export function approvalConfirmation(facts: ApprovalFacts): Confirmation | null {
  const toolName = facts.toolName.trim()
  if (toolName === '') return null
  const reason = facts.reason?.trim()
  const subject = reason === undefined || reason === '' ? toolName : `${toolName}：${reason}`
  const action = fitText(`批准运行 ${subject}`, CONFIRMATION_LIMITS.actionBytes)
  const params: Record<string, unknown> = { kind: 'approval', tool: toolName }
  if (facts.callId !== undefined && facts.callId !== '') params.callId = facts.callId
  if (reason !== undefined && reason !== '') params.reason = reason
  const args = argumentsOf(facts.argumentsJson)
  if (args !== undefined) params.arguments = args
  return { action, params: fitParams(params) }
}

/**
 * Build the confirmation object for a pending user question batch.
 * @param questions - the batch the host is waiting on.
 * @returns the confirmation, or `null` when the batch is empty.
 */
export function questionConfirmation(questions: readonly ConfirmationQuestion[]): Confirmation | null {
  if (questions.length === 0) return null
  const first = questions[0]
  if (first === undefined) return null
  const lead = first.header ?? first.question
  const count = questions.length === 1 ? '' : `（共 ${String(questions.length)} 个提问）`
  const action = fitText(`回答提问：${lead}${count}`, CONFIRMATION_LIMITS.actionBytes)
  const params: Record<string, unknown> = {
    kind: 'question',
    questions: questions.map((item) => {
      const entry: Record<string, unknown> = { id: item.id, question: item.question }
      if (item.header !== undefined) entry.header = item.header
      if (item.detail !== undefined) entry.detail = item.detail
      if (item.multiSelect === true) entry.multiSelect = true
      if (item.options !== undefined && item.options.length > 0) {
        entry.options = item.options.map((option) => {
          const shaped: Record<string, unknown> = { label: option.label }
          if (option.description !== undefined) shaped.description = option.description
          return shaped
        })
      }
      return entry
    }),
  }
  return { action, params: fitParams(params) }
}
