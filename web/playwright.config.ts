import { defineConfig, devices } from '@playwright/test'

/** The E2E origin; `scripts/e2e.sh` exports the matching port. */
const PORT = Number(process.env.E2E_PORT ?? 7444)
const ORIGIN = `http://localhost:${PORT}`

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  // One scratch server and one single-use setup token serve the whole run.
  workers: 1,
  // No retries: the setup ceremony consumes a single-use token from one
  // scratch server, so a retried serial group cannot start over.
  retries: 0,
  reporter: process.env.CI ? 'github' : 'line',
  use: {
    baseURL: ORIGIN,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
