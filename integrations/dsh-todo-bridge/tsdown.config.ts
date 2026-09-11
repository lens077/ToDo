import { defineConfig } from 'tsdown'

/**
 * One ESM host entry. Every `@deepseek-ai/*` import stays external: the
 * profile resolves them from the dsh installation closure at runtime (the
 * type-only ones are erased). Node builtins are external by platform.
 */
export default defineConfig({
  entry: ['src/index.ts'],
  outDir: 'lib',
  format: ['esm'],
  platform: 'node',
  target: 'es2024',
  fixedExtension: false,
  dts: true,
  sourcemap: true,
  clean: true,
  deps: { neverBundle: [/^@deepseek-ai\//u] },
})
