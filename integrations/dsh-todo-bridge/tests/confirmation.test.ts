import { Buffer } from 'node:buffer'
import { describe, expect, it } from 'vitest'
import { CONFIRMATION_LIMITS, approvalConfirmation, questionConfirmation } from '../src/confirmation.ts'

/** Byte length of a string as the contract measures it. */
const bytes = (text: string): number => Buffer.byteLength(text, 'utf8')

/** Parse the params string the way ToDo does before hashing it. */
const paramsOf = (params: string): Record<string, unknown> => JSON.parse(params) as Record<string, unknown>

describe('approvalConfirmation', () => {
  it('names the tool and carries the logged arguments', () => {
    const confirmation = approvalConfirmation({
      toolName: 'bash',
      callId: 'call-1',
      reason: '写入工作区外的文件',
      argumentsJson: '{"command":"rm -rf /tmp/x","cwd":"/tmp"}',
    })
    expect(confirmation).not.toBeNull()
    expect(confirmation?.action).toBe('批准运行 bash：写入工作区外的文件')
    const params = paramsOf(confirmation?.params ?? '{}')
    expect(params.kind).toBe('approval')
    expect(params.tool).toBe('bash')
    expect(params.callId).toBe('call-1')
    expect(params.reason).toBe('写入工作区外的文件')
    expect(params.arguments).toEqual({ command: 'rm -rf /tmp/x', cwd: '/tmp' })
  })

  it('omits arguments that are not a JSON object', () => {
    for (const argumentsJson of ['[1,2]', '"text"', '42', 'not-json', '']) {
      const confirmation = approvalConfirmation({ toolName: 'bash', argumentsJson })
      const params = paramsOf(confirmation?.params ?? '{}')
      expect(params.arguments).toBeUndefined()
      expect(params.kind).toBe('approval')
    }
  })

  it('keeps action within the contract byte cap', () => {
    const confirmation = approvalConfirmation({ toolName: 'bash', reason: '很长的理由'.repeat(120) })
    expect(bytes(confirmation?.action ?? '')).toBeLessThanOrEqual(CONFIRMATION_LIMITS.actionBytes)
    expect(confirmation?.action.endsWith('…')).toBe(true)
  })

  it('shrinks oversized arguments while keeping params a JSON object', () => {
    const confirmation = approvalConfirmation({
      toolName: 'write',
      argumentsJson: JSON.stringify({ path: '/tmp/big.txt', content: 'x'.repeat(20_000) }),
    })
    const params = paramsOf(confirmation?.params ?? 'null')
    expect(bytes(confirmation?.params ?? '')).toBeLessThanOrEqual(CONFIRMATION_LIMITS.paramsBytes)
    expect(params.kind).toBe('approval')
    expect(params.tool).toBe('write')
  })

  it('returns null for an unnamed tool', () => {
    expect(approvalConfirmation({ toolName: '   ' })).toBeNull()
  })

  it('serializes the same facts identically, so an approval keeps its digest', () => {
    const facts = { toolName: 'bash', callId: 'c1', argumentsJson: '{"a":1,"b":2}' } as const
    expect(approvalConfirmation(facts)).toEqual(approvalConfirmation(facts))
  })
})

describe('questionConfirmation', () => {
  it('carries every question with its options', () => {
    const confirmation = questionConfirmation([
      {
        id: 'q1',
        question: '选哪个方案？',
        header: '方案',
        options: [{ label: 'A', description: '快' }, { label: 'B' }],
        multiSelect: true,
      },
    ])
    expect(confirmation?.action).toBe('回答提问：方案')
    const params = paramsOf(confirmation?.params ?? '{}')
    expect(params.kind).toBe('question')
    expect(params.questions).toEqual([
      {
        id: 'q1',
        question: '选哪个方案？',
        header: '方案',
        multiSelect: true,
        options: [{ label: 'A', description: '快' }, { label: 'B' }],
      },
    ])
  })

  it('counts a batch and falls back to the first question text without a header', () => {
    const confirmation = questionConfirmation([
      { id: 'q1', question: '第一个问题？' },
      { id: 'q2', question: '第二个问题？' },
    ])
    expect(confirmation?.action).toBe('回答提问：第一个问题？（共 2 个提问）')
    expect((paramsOf(confirmation?.params ?? '{}').questions as unknown[]).length).toBe(2)
  })

  it('returns null for an empty batch', () => {
    expect(questionConfirmation([])).toBeNull()
  })

  it('keeps action and params within their caps', () => {
    const confirmation = questionConfirmation([
      { id: 'q1', question: '问题'.repeat(400), detail: '细节'.repeat(400) },
    ])
    expect(bytes(confirmation?.action ?? '')).toBeLessThanOrEqual(CONFIRMATION_LIMITS.actionBytes)
    expect(bytes(confirmation?.params ?? '')).toBeLessThanOrEqual(CONFIRMATION_LIMITS.paramsBytes)
  })
})
