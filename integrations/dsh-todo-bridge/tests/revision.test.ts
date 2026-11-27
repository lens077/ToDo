import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { BRIDGE_SOURCE_REVISION } from '../src/index.ts'

/** The manifest the installed package carries; drift here is what this test forbids. */
const manifest = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
) as { version: string }

describe('published revision', () => {
  it('names the version from package.json', () => {
    expect(BRIDGE_SOURCE_REVISION).toBe(`dsh-todo-bridge/${manifest.version}`)
  })
})
