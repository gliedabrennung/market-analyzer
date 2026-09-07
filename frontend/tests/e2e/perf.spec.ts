import { expect, test } from '@playwright/test'
import { buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

declare global {
  interface Window {
    __longTaskDurations: number[]
  }
}

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

test('renders 50,000 candles in under 200ms (NFR-1.3)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))

  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )

  const allCandles = makeFixtureCandles(50_000)
  const today = new Date().toISOString().slice(0, 10)

  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())

    if (!(url.searchParams.get('to') ?? '').startsWith(today)) {
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

test('panning the chart does not trigger a long task (NFR-1.4)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  const candles = makeFixtureCandles(5_000)
  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())

    if (!(url.searchParams.get('to') ?? '').startsWith(today)) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(candles)
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  const canvas = page.getByTestId('price-chart').locator('canvas').first()
  await expect(canvas).toBeVisible()

  await page.evaluate(() => {
    window.__longTaskDurations = []
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__longTaskDurations.push(entry.duration)
    }).observe({ entryTypes: ['longtask'] })
  })

  const box = await canvas.boundingBox()
  if (box === null) throw new Error('price chart has no bounding box')
  const y = box.y + box.height / 2
  await page.mouse.move(box.x + box.width * 0.75, y)
  await page.mouse.down()
  for (let i = 1; i <= 15; i++) {
    await page.mouse.move(box.x + box.width * 0.75 - i * 25, y)
  }
  await page.mouse.up()
  await page.waitForTimeout(200)

  const durations = await page.evaluate(() => window.__longTaskDurations)
  const worst = durations.length > 0 ? Math.max(...durations) : 0
  expect(worst).toBeLessThan(50)
})

test('holds close to 60fps under a 1000-message WS burst (NFR-1.5)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())

    if (!(url.searchParams.get('to') ?? '').startsWith(today)) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(makeFixtureCandles(200))
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })

  let sendTrade: ((frame: string) => void) | undefined
  await page.routeWebSocket('**/stream/**', (ws) => {
    sendTrade = (frame) => ws.send(frame)
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()
  await expect.poll(() => sendTrade !== undefined).toBe(true)

  const WINDOW_MS = 1000
  const fpsPromise = page.evaluate(async (windowMs) => {
    let frames = 0
    const start = performance.now()
    await new Promise<void>((resolve) => {
      function tick() {
        frames++
        if (performance.now() - start < windowMs) requestAnimationFrame(tick)
        else resolve()
      }
      requestAnimationFrame(tick)
    })
    return frames / (windowMs / 1000)
  }, WINDOW_MS)

  for (let i = 0; i < 1000; i++) {
    sendTrade?.(tradeFrame(i, '50000.00', i % 2 === 0))
  }

  const fps = await fpsPromise

  expect(fps).toBeGreaterThan(50)
})

test('heap growth stays bounded under a sustained trade flood (NFR-1.7, short proxy)', async ({
  page,
  context,
}) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())

    if (!(url.searchParams.get('to') ?? '').startsWith(today)) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(makeFixtureCandles(200))
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })

  let sendTrade: ((frame: string) => void) | undefined
  await page.routeWebSocket('**/stream/**', (ws) => {
    sendTrade = (frame) => ws.send(frame)
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()
  await expect.poll(() => sendTrade !== undefined).toBe(true)

  const cdp = await context.newCDPSession(page)
  await cdp.send('HeapProfiler.enable')
  async function usedHeapBytes(): Promise<number> {
    await cdp.send('HeapProfiler.collectGarbage')
    const usage = (await cdp.send('Runtime.getHeapUsage')) as { usedSize: number }
    return usage.usedSize
  }

  const before = await usedHeapBytes()

  const ROUNDS = 20
  const MSGS_PER_ROUND = 200
  for (let round = 0; round < ROUNDS; round++) {
    for (let i = 0; i < MSGS_PER_ROUND; i++) {
      sendTrade?.(tradeFrame(round * MSGS_PER_ROUND + i, '50000.00', i % 2 === 0))
    }
    await page.waitForTimeout(50)
  }

  const after = await usedHeapBytes()
  const growthRatio = (after - before) / before
  expect(growthRatio).toBeLessThan(0.5)
})

test('switching back to a cached interval fires no new request (NFR-1.8)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )

  const requestCounts: Record<string, number> = {}
  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())
    const interval = url.searchParams.get('interval') ?? ''
    requestCounts[interval] = (requestCounts[interval] ?? 0) + 1

    if (!(url.searchParams.get('to') ?? '').startsWith(today)) {
      route.fulfill({ contentType: 'application/json', body: '[]' })
      return
    }
    const bytes = buildOhlcvArrowIpc(makeFixtureCandles(200))
    route.fulfill({ contentType: 'application/vnd.apache.arrow.stream', body: Buffer.from(bytes) })
  })

  await page.goto('/?symbol=BTCUSDT&interval=1m')
  await expect(page.getByTestId('price-chart').locator('canvas').first()).toBeVisible()
  await expect.poll(() => requestCounts['1m'] ?? 0).toBeGreaterThan(0)
  const firstVisitRequests = requestCounts['1m']

  await page.getByRole('button', { name: '5m', exact: true }).click()
  await expect.poll(() => new URL(page.url()).searchParams.get('interval')).toBe('5m')
  await expect.poll(() => requestCounts['5m'] ?? 0).toBeGreaterThan(0)

  await page.getByRole('button', { name: '1m', exact: true }).click()
  await expect.poll(() => new URL(page.url()).searchParams.get('interval')).toBe('1m')

  await page.waitForTimeout(300)

  expect(requestCounts['1m']).toBe(firstVisitRequests)
})
