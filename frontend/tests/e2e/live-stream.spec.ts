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
    const to = new URL(route.request().url()).searchParams.get('to')

    if (to === null || !to.startsWith(today)) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(makeFixtureCandles(200))
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })
})

function tradeFrame(tradeId: number, price: string, isBuyerMaker: boolean): string {
  return JSON.stringify({
    type: 'trade',
    ts: new Date().toISOString(),
    symbol: 'BTCUSDT',
    exchange: 'binance',
    trade_id: tradeId,
    price,
    qty: '0.001',
    is_buyer_maker: isBuyerMaker,
  })
}

test('shows live trades in the trade tape (FR-7.1)', async ({ page }) => {
  await page.routeWebSocket('**/stream/**', (ws) => {
    ws.send(tradeFrame(1, '50000.00', false))
    ws.send(tradeFrame(2, '50001.50', true))
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  await expect(page.getByText('50001.50')).toBeVisible()
  await expect(page.getByText('50000.00')).toBeVisible()
})

test('reconnects with backoff after a forced disconnect (FR-2.2)', async ({ page }) => {
  let connectionCount = 0

  await page.routeWebSocket('**/stream/**', (ws) => {
    connectionCount++
    if (connectionCount === 1) {
      setTimeout(() => ws.close(), 200)
    } else {
      ws.send(tradeFrame(99, '51234.00', false))
    }
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()

  await expect.poll(() => connectionCount, { timeout: 5000 }).toBeGreaterThanOrEqual(2)

  await expect(page.getByText('51234.00')).toBeVisible()
  await expect(page.getByText('Live')).toBeVisible()
})

test('a live kline tick updates the chart via update(), not a full setData() re-render (FR-3.3)', async ({
  page,
}) => {
  let sendKline: ((frame: string) => void) | undefined
  await page.routeWebSocket('**/stream/**', (ws) => {
    sendKline = (frame) => ws.send(frame)
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()
  await expect.poll(() => sendKline !== undefined).toBe(true)

  const setDataCallsBefore = await page.evaluate(
    () => performance.getEntriesByName('price-chart:set-data').length,
  )

  const openTime = new Date(Math.floor(Date.now() / 60_000) * 60_000)
  sendKline?.(
    JSON.stringify({
      type: 'kline',
      open_time: openTime.toISOString(),
      close_time: new Date(openTime.getTime() + 59_999).toISOString(),
      symbol: 'BTCUSDT',
      exchange: 'binance',
      interval: '1m',
      open: '50000.00000000',
      high: '50010.00000000',
      low: '49990.00000000',
      close: '50005.00000000',
      volume: '1.00000000',
      quote_volume: '50005.00000000',
      trades_count: 5,
      is_closed: false,
    }),
  )

  await page.waitForTimeout(300)

  const setDataCallsAfter = await page.evaluate(
    () => performance.getEntriesByName('price-chart:set-data').length,
  )
  expect(setDataCallsAfter).toBe(setDataCallsBefore)
})

test('ignores a live kline for a pair other than the one on screen', async ({ page }) => {
  let sendKline: ((frame: string) => void) | undefined
  await page.routeWebSocket('**/stream/**', (ws) => {
    sendKline = (frame) => ws.send(frame)
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()
  await expect.poll(() => sendKline !== undefined).toBe(true)

  const openTime = new Date(Math.floor(Date.now() / 60_000) * 60_000)
  const klineFrame = (symbol: string, interval: string, price: string) =>
    JSON.stringify({
      type: 'kline',
      open_time: openTime.toISOString(),
      close_time: new Date(openTime.getTime() + 59_999).toISOString(),
      symbol,
      exchange: 'binance',
      interval,
      open: price,
      high: price,
      low: price,
      close: price,
      volume: '1.00000000',
      quote_volume: price,
      trades_count: 5,
      is_closed: false,
    })

  sendKline?.(klineFrame('ETHUSDT', '1m', '2500.00000000'))
  sendKline?.(klineFrame('BTCUSDT', '5m', '999999.00000000'))
  await page.waitForTimeout(400)

  await expect(page.getByText('2500.00', { exact: false })).toHaveCount(0)
  await expect(page.getByText('999999.00', { exact: false })).toHaveCount(0)

  sendKline?.(klineFrame('BTCUSDT', '1m', '50123.00000000'))
  await expect(page.getByText('50123.00', { exact: false }).first()).toBeVisible()
})
