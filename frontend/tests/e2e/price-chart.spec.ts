import { expect, test } from '@playwright/test'
import { buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

const SYMBOLS_JSON = [
  {
    exchange: 'binance',
    symbol: 'BTCUSDT',
    base_asset: 'BTC',
    quote_asset: 'USDT',
    status: 'TRADING',
    has_data: true,
  },
]

test.beforeEach(async ({ page }) => {
  await page.route('**/symbols', (route) =>
    route.fulfill({ contentType: 'application/json', body: JSON.stringify(SYMBOLS_JSON) }),
  )

  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )

  const today = new Date().toISOString().slice(0, 10)

  await page.route('**/ohlcv/**', (route) => {
    const accept = route.request().headers()['accept'] ?? ''

    const to = new URL(route.request().url()).searchParams.get('to')

    if (to === null || !to.startsWith(today)) {
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

test('loads candles via Arrow and reflects selection in the URL', async ({ page }) => {
  const arrowRequest = page.waitForRequest(
    (req) => req.url().includes('/ohlcv/') && (req.headers()['accept'] ?? '').includes('arrow'),
  )

  await page.goto('/?symbol=BTCUSDT&interval=1m')

  const request = await arrowRequest
  const response = await request.response()
  expect(response?.headers()['content-type']).toContain('application/vnd.apache.arrow.stream')

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
