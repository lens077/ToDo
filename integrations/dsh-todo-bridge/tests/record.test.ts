import { Buffer } from 'node:buffer'
import { describe, expect, it } from 'vitest'
import { CONTRACT_LIMITS, buildRecord, contentKey, eventAtOf, relativeToWorkspace, truncateUtf8 } from '../src/record.ts'
import { NOW, WORKSPACE, digest, meta, openDigest, row } from './fixtures.ts'

describe('truncateUtf8', () => {
  it('leaves short text alone', () => {
    expect(truncateUtf8('abc', 3)).toEqual({ text: 'abc', truncated: false })
  })
  it('cuts by UTF-8 bytes on a code-point boundary', () => {
    // 3-byte CJK characters: 7 bytes admits two of them, never a torn third.
    expect(truncateUtf8('说明说明', 7)).toEqual({ text: '说明', truncated: true })
    // A 4-byte astral character does not fit in 3 bytes.
    expect(truncateUtf8('😀a', 3)).toEqual({ text: '', truncated: true })
    expect(truncateUtf8('a😀', 5)).toEqual({ text: 'a😀', truncated: false })
    expect(Buffer.byteLength(truncateUtf8('x'.repeat(5000), 4000).text)).toBe(4000)
  })
})

describe('relativeToWorkspace', () => {
  it('relativizes paths under the workspace and passes the rest through', () => {
    expect(relativeToWorkspace(`${WORKSPACE}/crates/todo-cli/src/main.rs`, WORKSPACE)).toBe('crates/todo-cli/src/main.rs')
    expect(relativeToWorkspace('/etc/hosts', WORKSPACE)).toBe('/etc/hosts')
    expect(relativeToWorkspace('README.md', WORKSPACE)).toBe('README.md')
    expect(relativeToWorkspace(WORKSPACE, WORKSPACE)).toBe(WORKSPACE)
    expect(relativeToWorkspace(`${WORKSPACE}-other/x`, WORKSPACE)).toBe(`${WORKSPACE}-other/x`)
  })

  it('accepts alias spellings of the workspace (realpath) as a prefix', () => {
    const real = '/private/tmp/ws'
    expect(relativeToWorkspace(`${real}/src/a.rs`, '/tmp/ws', [real])).toBe('src/a.rs')
    expect(relativeToWorkspace('/tmp/ws/src/a.rs', '/tmp/ws', [real])).toBe('src/a.rs')
    expect(relativeToWorkspace(real, '/tmp/ws', [real])).toBe(real)
  })

  it('falls back to the longest trailing workspace segment run found in the path', () => {
    // The observed case: the session log was written under another user's
    // home, while the workspace is now spelled with the current user.
    const workspace = '/Users/lens/lens077/kubernetes'
    expect(relativeToWorkspace('/Users/sumery/lens077/kubernetes/components/gateway/README.md', workspace))
      .toBe('components/gateway/README.md')
    // Longest run wins: `lens077/kubernetes` over a stray `kubernetes` earlier in the path.
    expect(relativeToWorkspace('/mnt/kubernetes/lens077/kubernetes/a.md', workspace)).toBe('a.md')
    // Basename alone is the last resort.
    expect(relativeToWorkspace('/srv/kubernetes/deploy/x.yaml', workspace)).toBe('deploy/x.yaml')
    // The first occurrence of the run is taken as the root.
    expect(relativeToWorkspace('/srv/kubernetes/vendor/kubernetes/x.yaml', workspace)).toBe('vendor/kubernetes/x.yaml')
    // A path that merely ends with the run is not a file inside the workspace.
    expect(relativeToWorkspace('/srv/lens077/kubernetes', workspace)).toBe('/srv/lens077/kubernetes')
    // Nothing in common: untouched.
    expect(relativeToWorkspace('/etc/hosts', workspace)).toBe('/etc/hosts')
    expect(relativeToWorkspace('/Users/lens/lens077/kubernetes-old/x', workspace)).toBe('/Users/lens/lens077/kubernetes-old/x')
  })

  it('buildRecord passes workspace aliases through', () => {
    const record = buildRecord({
      meta: meta({ workspace: '/tmp/ws' }),
      digest: digest({ changedFiles: ['/private/tmp/ws/a.rs', '/Users/x/tmp/ws/b.rs', '/etc/hosts'] }),
      row: undefined,
      state: 'unread',
      openUrl: null,
      workspaceAliases: ['/private/tmp/ws'],
    })
    expect(record.changed_files).toEqual(['a.rs', 'b.rs', '/etc/hosts'])
  })
})

describe('eventAtOf', () => {
  it('takes the newest source-side time, with creation as the floor', () => {
    expect(eventAtOf({ meta: meta(), digest: undefined, row: undefined })).toBe(NOW - 120_000)
    expect(eventAtOf({ meta: meta(), digest: digest(), row: undefined })).toBe(NOW - 30_000)
    expect(eventAtOf({ meta: meta(), digest: digest(), row: row({ updatedAt: NOW - 5 }) })).toBe(NOW - 5)
    expect(eventAtOf({ meta: meta({ signalAt: NOW }), digest: digest(), row: row({ updatedAt: NOW - 5 }) })).toBe(NOW)
  })
})

describe('buildRecord', () => {
  it('builds the §7 example shape', () => {
    const record = buildRecord({ meta: meta(), digest: digest(), row: undefined, state: 'unread', openUrl: 'http://127.0.0.1:3080' })
    expect(record).toEqual({
      'ai-feed-spec': 1,
      source: 'dsh',
      session_id: 's_8f2a',
      workspace: WORKSPACE,
      state: 'unread',
      outcome: 'completed',
      title: '文档｜cargo install 说明',
      question: '补一段 cargo install 的说明',
      reply: '已在 README 快速开始加入 cargo install --path …',
      reply_truncated: false,
      changed_files: ['README.md'],
      changed_file_count: 1,
      open_url: 'http://127.0.0.1:3080',
      event_at: NOW - 30_000,
    })
    expect(JSON.parse(JSON.stringify(record))).toEqual(record)
  })

  it('nulls the outcome while the turn is open (running / waiting)', () => {
    const stale = digest({ outcome: 'error' })
    expect(buildRecord({ meta: meta({ running: true }), digest: stale, row: undefined, state: 'running', openUrl: null }).outcome).toBeNull()
    expect(buildRecord({ meta: meta({ waiting: true }), digest: stale, row: undefined, state: 'waiting', openUrl: null }).outcome).toBeNull()
    expect(buildRecord({ meta: meta(), digest: openDigest(), row: undefined, state: 'failed', openUrl: null }).outcome).toBeNull()
  })

  it('tolerates a missing digest', () => {
    const record = buildRecord({ meta: meta({ running: true }), digest: undefined, row: undefined, state: 'running', openUrl: null })
    expect(record.question).toBeNull()
    expect(record.reply).toBeNull()
    expect(record.changed_files).toEqual([])
    expect(record.changed_file_count).toBe(0)
    expect(record.event_at).toBe(NOW - 120_000)
  })

  it('truncates every bounded field and flags a cut reply', () => {
    const long = digest({
      question: 'q'.repeat(CONTRACT_LIMITS.questionBytes + 10),
      reply: 'r'.repeat(CONTRACT_LIMITS.replyBytes + 10),
      changedFiles: Array.from({ length: 40 }, (_, index) => `${WORKSPACE}/f${String(index)}.ts`),
      changedFileCount: 40,
    })
    const record = buildRecord({
      meta: meta({ title: 't'.repeat(CONTRACT_LIMITS.titleBytes + 10), sessionId: 's'.repeat(200) }),
      digest: long,
      row: undefined,
      state: 'unread',
      openUrl: null,
    })
    expect(Buffer.byteLength(record.question ?? '')).toBe(CONTRACT_LIMITS.questionBytes)
    expect(Buffer.byteLength(record.reply ?? '')).toBe(CONTRACT_LIMITS.replyBytes)
    expect(Buffer.byteLength(record.title ?? '')).toBe(CONTRACT_LIMITS.titleBytes)
    expect(Buffer.byteLength(record.session_id)).toBe(CONTRACT_LIMITS.sessionIdBytes)
    expect(record.reply_truncated).toBe(true)
    expect(record.changed_files).toHaveLength(CONTRACT_LIMITS.changedFiles)
    expect(record.changed_files[0]).toBe('f0.ts')
    expect(record.changed_file_count).toBe(40)
  })

  it('keeps the host truncation flag when the host already cut the reply', () => {
    const record = buildRecord({ meta: meta(), digest: digest({ replyTruncated: true }), row: undefined, state: 'unread', openUrl: null })
    expect(record.reply_truncated).toBe(true)
  })

  it('refuses a session without a workspace', () => {
    expect(() => buildRecord({ meta: meta({ workspace: null }), digest: digest(), row: undefined, state: 'unread', openUrl: null }))
      .toThrow(/no workspace/u)
  })
})

describe('contentKey', () => {
  it('ignores event_at and nothing else', () => {
    const base = buildRecord({ meta: meta(), digest: digest(), row: undefined, state: 'unread', openUrl: null })
    const later = buildRecord({ meta: meta({ signalAt: NOW + 1 }), digest: digest(), row: undefined, state: 'unread', openUrl: null })
    const seen = buildRecord({ meta: meta(), digest: digest(), row: undefined, state: 'seen', openUrl: null })
    expect(later.event_at).not.toBe(base.event_at)
    expect(contentKey(later)).toBe(contentKey(base))
    expect(contentKey(seen)).not.toBe(contentKey(base))
  })
})
