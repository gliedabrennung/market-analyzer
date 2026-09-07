import { expect, test } from '@playwright/test'
import { buildAnomaliesArrowIpc, buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

const SYMBOLS_JSON = [
  {
    exchange: 'binance',
    symbol: 'BTCUSDT',
    base_asset: 'BTC',
    quote_asset: 'USDT',
    status: 'TRADING',
    has_data: true,
  },
  {
    exchange: 'binance',
    symbol: 'ETHUSDT',
    base_asset: 'ETH',
    quote_asset: 'USDT',
    status: 'TRADING',
    has_data: true,
  },
]

const candles = makeFixtureCandles(200)
// A bar comfortably inside the fixture range, for the anomaly's openTime
// — needs to line up with a real candle so AnomaliesTable's `priceByTime`
// lookup (app.tsx) finds a price, not "—".
const ANOMALY_OPEN_TIME = candles[50]!.openTime

test.beforeEach(async ({ page }) => {
  await page.route('**/symbols', (route) =>
    route.fulfill({ contentType: 'application/json', body: JSON.stringify(SYMBOLS_JSON) }),
  )

  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const to = new URL(route.request().url()).searchParams.get('to')
    // The live window's `to` is an instant (see app.tsx's `range`), so it
    // starts with today's date; an older window carries an earlier date.
    if (to === null || !to.startsWith(today)) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(candles)
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })

  await page.route('**/analytics/*/anomalies**', (route) => {
    const bytes = buildAnomaliesArrowIpc([
      { openTime: ANOMALY_OPEN_TIME, volume: '42.00000000', zScore: 5.5 },
    ])
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })
  await page.route('**/analytics/*/vwap**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  await page.route('**/analytics/*/volatility**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  await page.route('**/analytics/*/ofi**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  await page.route('**/analytics/correlation**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
})

// FR-5.3: clicking an anomalies-table row centers the chart on that bar.
test('clicking an anomaly row jumps the chart to that bar', async ({ page }) => {
  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  await page.getByRole('button', { name: 'Аномалии' }).click()
  const row = page.getByText('5.50')
  await expect(row).toBeVisible()
  await row.click()

  await expect(page.getByTestId('price-chart')).toHaveAttribute('data-last-jump-nonce', /\d+/)
})

// FR-1.5: one endpoint failing (500) doesn't take any other panel down.
test('one failing analytics endpoint does not break the others', async ({ page }) => {
  await page.route('**/analytics/*/vwap**', (route) =>
    route.fulfill({
      status: 500,
      contentType: 'application/json',
      body: JSON.stringify({ error: { code: 'internal_error', message: 'boom', details: {} } }),
    }),
  )

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  // The main chart (candles + volume) still renders even though VWAP's
  // own fetch failed.
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()
  // The volatility panel (a fully separate query) still renders too.
  await expect(page.getByText('Realized Volatility')).toBeVisible()
})

// FR-8.5 hotkeys.
test('hotkeys: 1-6 switch interval, / focuses symbol search, Esc blurs it', async ({ page }) => {
  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  await page.keyboard.press('3')
  await expect.poll(() => new URL(page.url()).searchParams.get('interval')).toBe('15m')

  await page.keyboard.press('/')
  await expect(page.locator('[data-hotkey-target="symbol-search"]')).toBeFocused()

  await page.keyboard.press('Escape')
  await expect(page.locator('[data-hotkey-target="symbol-search"]')).not.toBeFocused()
})

test('hotkeys do not fire while typing in an input', async ({ page }) => {
  // Start on a *different* interval than what typing "1" would hotkey to
  // (1m) — otherwise a broken guard and a working one look identical.
  await page.goto('/?symbol=BTCUSDT&interval=15m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  const search = page.locator('[data-hotkey-target="symbol-search"]')
  await search.click()
  await search.pressSequentially('ETH1')

  expect(new URL(page.url()).searchParams.get('interval')).toBe('15m')
})
