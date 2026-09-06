import { defineConfig } from 'vitest/config'

// A separate file from vite.config.ts, not a merged `test` field there:
// vitest 3.x's bundled Vite types and this project's own vite@8 disagree
// at the type level on `Plugin` (a version-skew between the two, not a
// real config conflict — Vitest still merges this with vite.config.ts's
// plugins/server settings at runtime). Once vitest catches up, this can
// fold back into vite.config.ts.
export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./tests/setup.ts'],
    exclude: ['node_modules/**', 'tests/e2e/**'],
  },
})
