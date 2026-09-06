import { defineConfig, devices } from '@playwright/test'

// NFR-1.x budgets (frontend-tz.md §6.1) are production-build numbers —
// same reasoning as NFR-1.2 specifying Lighthouse CI, which always audits
// built output, never the dev server. A dev server is ~2x slower here
// (unbundled ESM, unminified) purely from transport/parse overhead
// unrelated to the app's own performance, so `perf.spec.ts` runs against
// a real `vite build` + `vite preview` while every other e2e spec uses
// the dev server for fast iteration.
export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  retries: 0,
  reporter: 'line',
  use: {
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], baseURL: 'http://localhost:5173' },
      testIgnore: '**/perf.spec.ts',
    },
    {
      name: 'chromium-perf',
      use: { ...devices['Desktop Chrome'], baseURL: 'http://localhost:4173' },
      testMatch: '**/perf.spec.ts',
    },
  ],
  webServer: [
    {
      command: 'npm run dev',
      url: 'http://localhost:5173',
      reuseExistingServer: !process.env.CI,
    },
    {
      command: 'npm run build && npm run preview -- --port 4173',
      url: 'http://localhost:4173',
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  ],
})
