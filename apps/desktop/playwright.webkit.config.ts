import { defineConfig, devices } from '@playwright/test';
import base from './playwright.config';

// Opt-in renderer coverage for the native desktop's engine family. This is not
// an assertion that the installed Tauri WKWebView itself was driven by Playwright.
export default defineConfig({
  ...base,
  outputDir: 'test-results-webkit',
  projects: [{
    name: 'webkit',
    testMatch: /(?:column-setup(?:-native|-live)?|task-menu-layout)\.spec\.ts/,
    use: { ...devices['Desktop Safari'] },
  }],
});
