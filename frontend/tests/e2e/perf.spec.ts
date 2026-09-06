import { expect, test } from '@playwright/test'
import { buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

// NFR-1.3 (frontend-tz.md §6.1): first render of 50,000 candles must land
// under 200ms, measured via `performance.mark`/`measure` bracketing
// `src/charts/price-chart.tsx`'s `setData()` calls. Reads the marks
// directly (not the dev-only console log — see `playwright.config.ts`,
// this spec runs against a production build, where that log is stripped).
test('renders 50,000 candles in under 200ms (NFR-1.3)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  // The main chart's overlays/panels each fetch independently (FR-4.2/4.3)
  // — keep them out of this test's way with a fast, empty response.
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )

  const allCandles = makeFixtureCandles(50_000)
  const today = new Date().toISOString().slice(0, 10)
  // `fetchOhlcv` now paginates fully (endpoints.ts's `fetchAllPages`) since
  // a single request is capped server-side at 10,000 rows — this mock
  // must honor `limit`/`offset` the same way the real backend does, or
  // the pagination loop never sees a short final page and never stops.
  //
  // Fitting all 50,000 candles on screen also puts the initial view near
  // bar 0, which correctly fires FR-3.5's load-earlier-history fetch (a
  // second, earlier-dated request) — respond to that with an empty page
  // (`to` won't be `today`, see below), same reasoning as
  // price-chart.spec.ts, or the same 50,000 rows would get prepended a
  // second time, out of order.
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())
    if (url.searchParams.get('to') !== today) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const limit = Number(url.searchParams.get('limit'))
    const offset = Number(url.searchParams.get('offset'))
    const page_ = allCandles.slice(offset, offset + limit)
    const bytes = buildOhlcvArrowIpc(page_)
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

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
