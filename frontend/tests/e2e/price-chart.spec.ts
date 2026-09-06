import { expect, test } from '@playwright/test'
import { buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

const SYMBOLS_JSON = [
  { exchange: 'binance', symbol: 'BTCUSDT', base_asset: 'BTC', quote_asset: 'USDT', status: 'TRADING' },
]

test.beforeEach(async ({ page }) => {
  await page.route('**/symbols', (route) =>
    route.fulfill({ contentType: 'application/json', body: JSON.stringify(SYMBOLS_JSON) }),
  )
  // The overlays/panels (FR-3.4/4.1) fetch independently of the main
  // chart's data — keep them out of these tests' way.
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )

  const today = new Date().toISOString().slice(0, 10)

  await page.route('**/ohlcv/**', (route) => {
    const accept = route.request().headers()['accept'] ?? ''
    // FR-3.5 fires a second, earlier-dated fetch on any view that starts
    // near the beginning of loaded data — true here, since 500 fixture
    // candles all fit on screen at once. A real backend would return
    // nothing for a range before its earliest data; mimic that (rather
    // than serving the same 500 candles again under an earlier `to`,
    // which would duplicate their timestamps out of order once prepended).
    const to = new URL(route.request().url()).searchParams.get('to')
    if (to !== today) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    if (!accept.includes('application/vnd.apache.arrow.stream')) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(makeFixtureCandles(500))
    route.fulfill({
      contentType: 'application/vnd.apache.arrow.stream',
      body: Buffer.from(bytes),
    })
  })
})

// Этап 1 acceptance (frontend-tz.md §8): candles render, data arrives via
// Arrow, URL reflects symbol/interval.
test('loads candles via Arrow and reflects selection in the URL', async ({ page }) => {
  const arrowRequest = page.waitForRequest(
    (req) => req.url().includes('/ohlcv/') && (req.headers()['accept'] ?? '').includes('arrow'),
  )

  await page.goto('/?symbol=BTCUSDT&interval=1m')

  const request = await arrowRequest
  const response = await request.response()
  expect(response?.headers()['content-type']).toContain('application/vnd.apache.arrow.stream')

  // Lightweight Charts renders onto a <canvas> inside our chart container.
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  const url = new URL(page.url())
  expect(url.searchParams.get('symbol')).toBe('BTCUSDT')
  expect(url.searchParams.get('interval')).toBe('1m')
})

test('switching interval updates the URL without a full reload', async ({ page }) => {
  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  await page.getByRole('button', { name: '5m', exact: true }).click()

  await expect.poll(() => new URL(page.url()).searchParams.get('interval')).toBe('5m')
})
