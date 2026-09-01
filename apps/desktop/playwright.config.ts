import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests run against the built frontend in its mock mode.
 *
 * Outside a Tauri runtime the bridge serves fixtures instead of calling Rust, so
 * these tests cover the interaction layer — view routing, the confirmation gate,
 * diff rendering — without needing a webview or a real project on disk. The Rust
 * side has its own contract tests; this is deliberately the other half.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [['github'], ['list']] : 'list',
  use: {
    baseURL: 'http://127.0.0.1:4173',
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
    command: 'pnpm vite preview --port 4173 --strictPort --host 127.0.0.1',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
