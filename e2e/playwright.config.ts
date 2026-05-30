import { defineConfig, devices } from '@playwright/test';

const PORT = 4319;

/**
 * Playwright E2E suite for tenrankai.
 *
 * Boots the real Rust server against an isolated fixture gallery
 * (e2e/fixtures) and exercises gallery display + image ordering through a
 * headless browser. Frontend assets must be built first (`npm run build`);
 * CI does this as a separate step.
 */
export default defineConfig({
  testDir: './tests',
  globalSetup: './global-setup.ts',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : 'list',
  timeout: 30_000,
  expect: { timeout: 10_000 },
  // Test artifacts (failure screenshots, traces) land here; the explicit
  // per-scenario screenshots are written to e2e/screenshots by the tests.
  outputDir: './test-results',

  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: 'on-first-retry',
    screenshot: 'on',
  },

  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],

  webServer: {
    // No AVIF needed for PNG fixtures; --no-default-features keeps the build fast.
    command:
      'cargo run --no-default-features -- serve --config e2e/fixtures/config.toml --log-level warn',
    cwd: '..',
    port: PORT,
    reuseExistingServer: !process.env.CI,
    timeout: 300_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
