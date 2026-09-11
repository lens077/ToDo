/**
 * The two ways the bridge starts the ToDo CLI: through the harness
 * `ctx.subprocess` seam when the composition mounts one (the host-plugin
 * convention, e.g. `dsh-host-open-in-app`), else through
 * `node:child_process`. Both feed the record on stdin and report exit facts.
 * @module dsh-todo-bridge/spawn
 */

import { spawn as nodeSpawn } from 'node:child_process'
import { homedir } from 'node:os'
import type { SubprocessRuntime } from '@deepseek-ai/dsh-subprocess'
import type { Spawner } from './publisher.ts'

/** Options shared by both launchers. */
export interface SpawnerOptions {
  /** Kill the CLI after this many milliseconds; it normally exits well under a second. */
  readonly timeoutMs?: number
  /** Working directory of the CLI. `todo ai` is global, so any existing directory works. */
  readonly cwd?: string
}

const DEFAULT_TIMEOUT_MS = 15_000
/** Bytes of stderr kept for the log line. */
const STDERR_MAX_BYTES = 8 * 1024

/**
 * Launcher over the harness subprocess seam. The executable is resolved once
 * on the seam's PATH and cached; a failed start clears the cache so a later
 * install of the CLI is picked up.
 * @param subprocess - the composition's `ctx.subprocess`.
 * @param options - deadline and working directory.
 * @returns a {@link Spawner}.
 */
export function createSubprocessSpawner(subprocess: SubprocessRuntime, options: SpawnerOptions = {}): Spawner {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS
  const cwd = options.cwd ?? homedir()
  let resolved: Promise<string> | undefined
  return async (argv, stdin) => {
    const [program, ...args] = argv
    if (program === undefined) throw new Error('todo-bridge: empty argv')
    resolved ??= subprocess.resolveExecutable(program)
    let executable: string
    try {
      executable = await resolved
    } catch (error) {
      resolved = undefined
      throw error
    }
    const controller = new AbortController()
    const deadline = setTimeout(() => { controller.abort() }, timeoutMs)
    deadline.unref?.()
    try {
      const handle = subprocess.spawn({
        argv: [executable, ...args],
        cwd,
        stdio: {
          stdin: { data: stdin },
          stdout: { maxBytes: STDERR_MAX_BYTES },
          stderr: { maxBytes: STDERR_MAX_BYTES },
        },
        graceMs: 2_000,
        signal: controller.signal,
      })
      const outcome = await handle.done
      const stderr = handle.collected.stderr?.readFrom(0).text ?? ''
      return {
        exitCode: controller.signal.aborted ? null : outcome.exitCode,
        stderr: controller.signal.aborted ? `${stderr}\n(timed out after ${String(timeoutMs)} ms)` : stderr,
      }
    } catch (error) {
      resolved = undefined
      throw error
    } finally {
      clearTimeout(deadline)
    }
  }
}

/**
 * Launcher over `node:child_process` for compositions without a subprocess
 * seam. Inherits the harness environment (PATH lookup happens in the OS).
 * @param options - deadline and working directory.
 * @returns a {@link Spawner}.
 */
export function createNodeSpawner(options: SpawnerOptions = {}): Spawner {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS
  const cwd = options.cwd ?? homedir()
  return (argv, stdin) => new Promise((resolve, reject) => {
    const [program, ...args] = argv
    if (program === undefined) {
      reject(new Error('todo-bridge: empty argv'))
      return
    }
    const child = nodeSpawn(program, args, { cwd, stdio: ['pipe', 'ignore', 'pipe'], windowsHide: true })
    let stderr = ''
    let timedOut = false
    const deadline = setTimeout(() => {
      timedOut = true
      child.kill('SIGKILL')
    }, timeoutMs)
    deadline.unref?.()
    child.stderr?.setEncoding('utf8')
    child.stderr?.on('data', (chunk: string) => {
      if (stderr.length < STDERR_MAX_BYTES) stderr += chunk
    })
    child.once('error', (error) => {
      clearTimeout(deadline)
      reject(error)
    })
    child.once('close', (code) => {
      clearTimeout(deadline)
      resolve({
        exitCode: timedOut ? null : code,
        stderr: timedOut ? `${stderr}\n(timed out after ${String(timeoutMs)} ms)` : stderr,
      })
    })
    // EPIPE on a child that exits before reading stdin is not a publish failure by itself.
    child.stdin?.on('error', () => undefined)
    child.stdin?.end(stdin)
  })
}
