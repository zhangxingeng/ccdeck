import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright E2E config for CC Deck.
 *
 * Drives the app the same way a user in a plain browser would: `src/lib/api.ts`
 * already has a complete browser-dev mock layer (see its module doc), so
 * `pnpm dev` alone serves deterministic fixture data with zero Tauri backend.
 * No separate test-mode seam is needed — specs just point at the dev server.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:1420',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
