import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { Publisher, UPSERT_ARGS } from '../src/publisher.ts'
import { buildRecord } from '../src/record.ts'
import { createNodeSpawner } from '../src/spawn.ts'
import { NOW, digest, meta } from './fixtures.ts'

/** The sibling ToDo checkout's debug or release CLI, when either is built. */
function findTodoBinary(): string | undefined {
  const root = fileURLToPath(new URL('../../..', import.meta.url))
  for (const candidate of [join(root, 'target/debug/todo'), join(root, 'target/release/todo')]) {
    if (!existsSync(candidate)) continue
    const probe = spawnSync(candidate, ['ai', '--help'], { encoding: 'utf8' })
    if (probe.status === 0) return candidate
  }
  return undefined
}

const TODO = findTodoBinary()

describe('createNodeSpawner', () => {
  it('rejects when the binary does not exist', async () => {
    const spawn = createNodeSpawner({ cwd: tmpdir() })
    await expect(spawn(['/nonexistent/todo-bridge-binary', ...UPSERT_ARGS], '{}')).rejects.toThrow(/ENOENT/u)
  })

  it('reports a non-zero exit with stderr', async () => {
    const spawn = createNodeSpawner({ cwd: tmpdir() })
    const outcome = await spawn([process.execPath, '-e', 'process.stderr.write("boom"); process.exit(3)'], '')
    expect(outcome).toEqual({ exitCode: 3, stderr: 'boom' })
  })

  it('feeds stdin and kills a hung process at the deadline', async () => {
    const spawn = createNodeSpawner({ cwd: tmpdir(), timeoutMs: 300 })
    const echo = await spawn([process.execPath, '-e', 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{process.stderr.write(s);process.exit(0)})'], 'hello')
    expect(echo).toEqual({ exitCode: 0, stderr: 'hello' })
    const hung = await spawn([process.execPath, '-e', 'setInterval(()=>{}, 1000)'], '')
    expect(hung.exitCode).toBeNull()
    expect(hung.stderr).toMatch(/timed out after 300 ms/u)
  })
})

describe.skipIf(TODO === undefined)('end to end against the real `todo` CLI (isolated HOME)', () => {
  let home: string
  const env = (): NodeJS.ProcessEnv => ({ ...process.env, HOME: home })

  beforeAll(async () => {
    home = await mkdtemp(join(tmpdir(), 'dsh-todo-bridge-'))
  })
  afterAll(async () => {
    await rm(home, { recursive: true, force: true })
  })

  it('upserts a record through the publisher and lists it back in contract order', async () => {
    const binary = TODO as string
    // The node spawner inherits the process environment; point HOME at the sandbox for this test only.
    const previousHome = process.env.HOME
    process.env.HOME = home
    try {
      const publisher = new Publisher({
        spawn: createNodeSpawner({ cwd: home }),
        todoBinary: binary,
        debounceMs: 0,
        maxConsecutiveFailures: 3,
        logger: { info: () => undefined, warn: (message) => { throw new Error(message) } },
      })
      const base = { digest: digest(), row: undefined, openUrl: 'http://127.0.0.1:3080' }
      publisher.schedule(buildRecord({ ...base, meta: meta({ sessionId: 'e2e-unread' }), state: 'unread' }))
      publisher.schedule(buildRecord({ ...base, meta: meta({ sessionId: 'e2e-waiting', signalAt: NOW }), state: 'waiting' }))
      await publisher.flush()
      // An older event_at for the same session is ignored by the CLI (exit 0, unchanged).
      publisher.schedule(buildRecord({
        ...base, meta: meta({ sessionId: 'e2e-unread', createdAt: NOW - 999_999_999 }), digest: digest({ repliedAt: null, questionAt: null }), state: 'failed',
      }))
      await publisher.flush()
      expect(publisher.stats.published).toBe(3)
      expect(publisher.stats.failed).toBe(0)
    } finally {
      if (previousHome === undefined) delete process.env.HOME
      else process.env.HOME = previousHome
    }

    const list = spawnSync(binary, ['ai', 'list', '--json'], { encoding: 'utf8', env: env() })
    expect(list.status).toBe(0)
    const parsed = JSON.parse(list.stdout) as { ok: boolean; data: { sessions: Array<Record<string, unknown>> } }
    expect(parsed.ok).toBe(true)
    expect(parsed.data.sessions.map(row => [row.session_id, row.state])).toEqual([
      ['e2e-waiting', 'waiting'],
      ['e2e-unread', 'unread'],
    ])
    const unread = parsed.data.sessions.find(row => row.session_id === 'e2e-unread')
    expect(unread).toMatchObject({
      source: 'dsh',
      workspace: meta().workspace,
      outcome: 'completed',
      title: meta().title,
      changed_files: ['README.md'],
      changed_file_count: 1,
      open_url: 'http://127.0.0.1:3080',
      event_at: NOW - 30_000,
    })
  })

  it('rejects a record the CLI does not accept with exit 2 and reports it as a failure', async () => {
    const binary = TODO as string
    const previousHome = process.env.HOME
    process.env.HOME = home
    try {
      const lines: string[] = []
      const publisher = new Publisher({
        spawn: createNodeSpawner({ cwd: home }),
        todoBinary: binary,
        debounceMs: 0,
        maxConsecutiveFailures: 3,
        logger: { info: () => undefined, warn: (message) => { lines.push(message) } },
      })
      const record = buildRecord({ meta: meta({ sessionId: 'e2e-bad' }), digest: digest(), row: undefined, state: 'unread', openUrl: null })
      publisher.schedule({ ...record, state: 'bogus' as never })
      await publisher.flush()
      expect(publisher.stats.failed).toBe(1)
      expect(lines[0]).toMatch(/exited 2/u)
    } finally {
      if (previousHome === undefined) delete process.env.HOME
      else process.env.HOME = previousHome
    }
  })
})
