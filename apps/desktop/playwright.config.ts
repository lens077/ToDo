import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests run against the built frontend in its mock mode.
 *
 * Outside a Tauri runtime the bridge serves fixtures instead of calling Rust, so
 * these tests cover the interaction layer — view routing, the confirmation gate,
 * diff rendering — without needing a webview or a real project on disk. The Rust
 * side has its own contract tests; this is deliberately the other half.
 */
// 4173 is vite's default preview port, so a preview left running by another
// project on this machine would be picked up through reuseExistingServer and
// every spec would run against the wrong app. Use a port of our own, and let
// PLAYWRIGHT_PORT move it when even that collides.
const port = Number(process.env.PLAYWRIGHT_PORT ?? 4187);
const origin = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [['github'], ['list']] : 'list',
  use: {
    baseURL: origin,
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      testIgnore: /mobile\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'mobile',
      testMatch: /mobile\.spec\.ts/,
      use: { ...devices['iPhone 13'], browserName: 'chromium' },
    },
  ],
  webServer: {
    // Bind explicitly: vite preview listens on ::1 only by default, so a
    // 127.0.0.1 health check never resolves and the server looks dead.
    command: `pnpm vite preview --port ${port} --strictPort --host 127.0.0.1`,
    url: origin,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
