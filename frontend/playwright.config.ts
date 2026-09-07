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
      // Every test here measures wall-clock time or frame timing
      // (NFR-1.3/1.4/1.5) — running them as several concurrent worker
      // processes puts them in CPU contention with each other and
      // inflates every measurement (reproduced: NFR-1.3 alone measured
      // ~90ms, but measured 406ms — over its own 200ms budget — once 4
      // of this file's tests ran as parallel workers on the same
      // machine). `fullyParallel: false` makes this one file run
      // sequentially in a single worker; the `chromium` project above is
      // unaffected and still runs fully parallel.
      fullyParallel: false,
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
