import { expect, test } from '@playwright/test'
import { buildOhlcvArrowIpc, makeFixtureCandles } from './fixtures.ts'

// Scratch space for the PerformanceObserver callback in the NFR-1.4 test
// below — declared globally (not `as any`) so the `page.evaluate` source
// still type-checks against NFR-2.2 (`any` only in explicitly-marked
// interop spots).
declare global {
  interface Window {
    __longTaskDurations: number[]
  }
}

// FR-7.1's wire shape (frontend-tz.md BE-5), same as live-stream.spec.ts.
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

// NFR-1.4: panning/zooming must hold 60fps — measured as "no Long Task
// (>50ms) fires during the gesture," the same proxy the ТЗ names
// ("Performance-профиль Chrome, без длинных задач > 50 мс").
test('panning the chart does not trigger a long task (NFR-1.4)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  const candles = makeFixtureCandles(5_000)
  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())
    if (url.searchParams.get('to') !== today) {
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

// NFR-1.5: 1000 WS messages/sec must not drop frames — `stream/buffer.ts`'s
// rAF batching (FR-2.3) exists specifically for this. Fired as an instant
// burst rather than metered over a real second: that's a *harder* version
// of the same scenario (all backpressure hits in one animation-frame
// window), and it exercises the same batching path either way.
test('holds close to 60fps under a 1000-message WS burst (NFR-1.5)', async ({ page }) => {
  await page.route('**/symbols', (route) => route.fulfill({ contentType: 'application/json', body: '[]' }))
  await page.route('**/analytics/**', (route) =>
    route.fulfill({ contentType: 'application/json', body: '[]' }),
  )
  const today = new Date().toISOString().slice(0, 10)
  await page.route('**/ohlcv/**', (route) => {
    const url = new URL(route.request().url())
    if (url.searchParams.get('to') !== today) {
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
  // 60fps is the target; a generous floor accounts for headless/CI/dev-
  // machine scheduling noise unrelated to the app's own batching logic
  // (the same reasoning perf.spec.ts's sibling tests already apply).
  expect(fps).toBeGreaterThan(50)
})

// NFR-1.7: heap growth over a full 1-hour live session should stay under
// 15% — not something a test suite can run for real. This is a much
// shorter, generously-thresholded proxy: force GC, flood several thousand
// trades (far more than an hour of realistic traffic would deliver in
// this many seconds), force GC again, and check *retained* heap growth is
// bounded rather than scaling with message count — which is what FR-2.4's
// buffer cap (trade tape: 500 entries) should guarantee structurally. It
// can catch "something is unboundedly retained per message" but can't
// certify the literal 15%/hour figure — see frontend/README.md's NFR
// table for that caveat spelled out.
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
    if (url.searchParams.get('to') !== today) {
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

// NFR-1.8: re-selecting an interval whose data is already cached must not
// re-hit the network (`main.tsx`'s `staleTime: 30_000`).
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
    if (url.searchParams.get('to') !== today) {
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
  // Give a would-be refetch a moment to fire before asserting its absence.
  await page.waitForTimeout(300)

  expect(requestCounts['1m']).toBe(firstVisitRequests)
})
