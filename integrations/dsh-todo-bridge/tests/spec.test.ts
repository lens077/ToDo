import { describe, expect, it } from 'vitest'
import { SPEC_PROBE_RECORD, probeFeedSpec } from '../src/spec.ts'

/** A spawner that answers like one CLI build. */
const answering = (exitCode: number, stderr: string) =>
  async (): Promise<{ exitCode: number; stderr: string }> => ({ exitCode, stderr })

describe('probeFeedSpec', () => {
  it('reads the range a v2 build prints', async () => {
    await expect(probeFeedSpec(
      answering(2, 'invalid ai-feed record: unsupported ai-feed-spec 3; this build accepts 1-2'),
      'todo',
    )).resolves.toBe(2)
  })

  it('stays on spec 1 for a build that accepts only 1', async () => {
    await expect(probeFeedSpec(
      answering(2, 'invalid ai-feed record: unsupported ai-feed-spec 3; this build accepts 1'),
      'todo',
    )).resolves.toBe(1)
  })

  it('stays on spec 1 when the CLI cannot be started', async () => {
    await expect(probeFeedSpec(
      async () => { throw new Error('spawn ENOENT') },
      'todo',
    )).resolves.toBe(1)
  })

  it('probes with a spec no build accepts, so the probe cannot write a record', () => {
    expect(SPEC_PROBE_RECORD['ai-feed-spec']).toBe(3)
  })
})
