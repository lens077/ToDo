/**
 * Which `ai-feed-spec` this host's `todo` CLI accepts.
 *
 * Publishing a `2` that the CLI rejects would fail every `waiting` record and
 * pause publishing after the failure budget, so the bridge asks before it sends
 * one. The probe is a record with a spec no build accepts: every build rejects
 * it after validation, before any write, and its diagnostic names the range.
 * @module dsh-todo-bridge/spec
 */

import { UPSERT_ARGS } from './publisher.ts'
import type { Spawner } from './publisher.ts'

/** The newest spec this bridge can publish. */
export const BRIDGE_FEED_SPEC = 2

/**
 * Probe record: a spec no build supports, carrying the fields every build
 * requires so validation reaches the spec check instead of a field error.
 */
export const SPEC_PROBE_RECORD = Object.freeze({
  'ai-feed-spec': BRIDGE_FEED_SPEC + 1,
  source: 'dsh',
  session_id: 'dsh-todo-bridge-spec-probe',
  workspace: '/',
  state: 'running',
  event_at: 0,
})

/** A build that accepts the highest spec prints its range as `accepts 1-2`. */
const ACCEPTS_V2 = /accepts 1-2\b/u

/**
 * Ask the CLI which specs it accepts.
 *
 * A missing binary, a spawn failure, or any unrecognized diagnostic yields `1`:
 * that is the record shape every published build has accepted since v1, so the
 * bridge degrades to plain records instead of rejecting writes.
 * @param spawn - the process launcher the publisher also uses.
 * @param todoBinary - ToDo CLI name or absolute path.
 * @returns the highest spec the CLI accepts.
 */
export async function probeFeedSpec(spawn: Spawner, todoBinary: string): Promise<1 | 2> {
  try {
    const outcome = await spawn([todoBinary, ...UPSERT_ARGS], JSON.stringify(SPEC_PROBE_RECORD))
    return ACCEPTS_V2.test(outcome.stderr) ? BRIDGE_FEED_SPEC : 1
  } catch {
    // A CLI that cannot be started is the publisher's failure to report; a
    // probe must not add one of its own.
    return 1
  }
}
