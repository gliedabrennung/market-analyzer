import { expect, test } from '@playwright/test'
import { buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

// NFR-1.3 (frontend-tz.md §6.1): first render of 50,000 candles must land
// under 200ms, measured via `performance.mark`/`measure` bracketing
// `src/charts/price-chart.tsx`'s `setData()` calls. Reads the marks
// directly (not the dev-only console log — see `playwright.config.ts`,
// this spec runs against a production build, where that log is stripped).
test('renders 50,000 candles in under 200ms (NFR-1.3)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))

  const bytes = buildOhlcvArrowIpc(makeFixtureCandles(50_000))
  await page.route('**/ohlcv/**', (route) =>
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) }),
  )

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.locator('canvas').first()).toBeVisible()

  const durationMs = await page.evaluate(() => {
    const measure = performance.measure(
      'price-chart:set-data',
      'price-chart:set-data:start',
      'price-chart:set-data:end',
    )
    return measure.duration
  })
  expect(durationMs).toBeLessThan(200)
})
