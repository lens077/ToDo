import { readFileSync } from 'node:fs'
import { defineConfig } from 'tsdown'

/** Browser module id must equal the package name (`window.__ModuleLoader__.load({ id })`). */
const PACKAGE_NAME = (JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as { name: string }).name

/**
 * Two artifacts in one `lib/`:
 *
 * - `lib/index.js` — the ESM host entry. Every `@deepseek-ai/*` import stays
 *   external: the profile resolves them from the dsh installation closure at
 *   runtime (the type-only ones are erased). Node builtins are external by
 *   platform.
 * - `lib/client.js` — the browser client half in the factory-form CJS shape
 *   dsh-web's module loader consumes (the same banner / intro / footer the DSH
 *   repository's `clientBundle` preset emits). It only imports types, so
 *   nothing is required from the loader module table at runtime; `neverBundle`
 *   still keeps any future `@deepseek-ai/*` value import external instead of
 *   silently inlining a second runtime instance.
 *
 * `clean` stays off on both (tsdown builds configs concurrently; a clean on
 * one would race the other's output) — `pnpm build` removes `lib/` first.
 */
export default defineConfig([
  {
    name: 'host',
    entry: ['src/index.ts'],
    outDir: 'lib',
    format: ['esm'],
    platform: 'node',
    target: 'es2024',
    fixedExtension: false,
    dts: true,
    sourcemap: true,
    clean: false,
    deps: { neverBundle: [/^@deepseek-ai\//u] },
  },
  {
    name: 'client',
    entry: { client: 'src/client/index.ts' },
    outDir: 'lib',
    format: ['cjs'],
    platform: 'browser',
    target: 'es2022',
    fixedExtension: false,
    dts: false,
    sourcemap: true,
    clean: false,
    deps: { neverBundle: [/^@deepseek-ai\//u, /^react(?:\/|$)/u, /^react-dom(?:\/|$)/u] },
    outputOptions: {
      entryFileNames: 'client.js',
      sourcemapExcludeSources: false,
      banner: `window.__ModuleLoader__.load({ id: ${JSON.stringify(PACKAGE_NAME)}, factory: (require) => {`,
      intro: 'var module = { exports: {} }; var exports = module.exports;',
      footer: 'return module.exports; } });',
    },
  },
  {
    // Declarations for `exports["./client"].types` (kept separate: a dts
    // build shares neither the banner nor the CJS output shape).
    name: 'client-types',
    entry: { client: 'src/client/index.ts' },
    outDir: 'lib',
    format: ['esm'],
    platform: 'browser',
    target: 'es2022',
    fixedExtension: false,
    dts: { emitDtsOnly: true },
    clean: false,
    deps: { neverBundle: [/^@deepseek-ai\//u] },
  },
])
