import { createEffect, onCleanup, onMount } from 'solid-js'
import { unwrap } from 'solid-js/store'
import {
  CandlestickSeries,
  ColorType,
  HistogramSeries,
  LineSeries,
  createChart,
  type IChartApi,
  type ISeriesApi,
  type LogicalRange,
  type UTCTimestamp,
} from 'lightweight-charts'
import type { OhlcvRow, VwapPoint } from '../api/types'
import { simpleMovingAverage } from './indicators'
import { cssToken } from './theme'
import { origin, publishVisibleRange, trackUserGestures, visibleRange } from './sync'

const MA_WINDOW = 20
/** FR-3.5: trigger an earlier-history fetch once the visible window gets
 * this close to the start of the loaded data, not only exactly at bar 0 —
 * a fast fling-pan can jump several bars past the edge in one event. */
const LOAD_EARLIER_THRESHOLD_BARS = 20

export interface PriceChartProps {
  data: OhlcvRow[]
  vwapData?: VwapPoint[]
  showVwap?: boolean
  showMa?: boolean
  /** FR-3.3/Этап 3: the latest live kline for the current bar (open or
   * just-closed), applied via `series.update()` — never `setData()`, so a
   * 1000 msg/sec live feed (NFR-1.5) doesn't re-diff/re-render the whole
   * series on every tick. `null`/`undefined`: no live tick applied yet
   * (e.g. freshly connected, or the historical load hasn't landed yet). */
  liveKline?: OhlcvRow | null
  /** FR-3.5: called when the user has scrolled near the left edge of the
   * loaded data; the parent is responsible for fetching and prepending
   * more history to `data`. Never called again while a call is already
   * "in flight" from the parent's perspective — that debouncing is the
   * parent's job (it knows when its own fetch resolves), not this
   * component's. */
  onLoadEarlier?: () => void
}

function toCandlestickPoint(row: OhlcvRow) {
  return {
    time: (row.openTime / 1000) as UTCTimestamp,
    open: row.open,
    high: row.high,
    low: row.low,
    close: row.close,
  }
}

function toVolumePoint(row: OhlcvRow, upColor: string, downColor: string) {
  return {
    time: (row.openTime / 1000) as UTCTimestamp,
    value: row.volume,
    color: row.close >= row.open ? upColor : downColor,
  }
}

function toVwapPoint(point: VwapPoint) {
  return { time: (point.openTime / 1000) as UTCTimestamp, value: point.vwap }
}

/** Same bar count and order as `rows`; `null` entries become gaps in the
 * line (Lightweight Charts skips a point whose `value` isn't finite). */
function toMaSeries(rows: OhlcvRow[]) {
  const ma = simpleMovingAverage(rows, MA_WINDOW)
  return rows
    .map((row, i) => ({ time: (row.openTime / 1000) as UTCTimestamp, value: ma[i] }))
    .filter((point): point is { time: UTCTimestamp; value: number } => point.value !== null)
}

/** FR-3.1/FR-3.2: candlestick + volume, panning/zoom/crosshair are
 * Lightweight Charts defaults. FR-3.4: optional VWAP/MA(20) line overlays.
 * FR-3.5: scrolling near the left edge asks the parent for more history.
 * FR-4.2: this chart's visible range is published to/synced from every
 * other chart via `./sync`. FR-3.3: `props.liveKline` updates the last
 * bar via `series.update()`, entirely separate from the historical
 * `props.data` → `setData()` path below. */
export function PriceChart(props: PriceChartProps) {
  const chartId = Symbol('price-chart')
  let container: HTMLDivElement | undefined
  let chart: IChartApi | undefined
  let candleSeries: ISeriesApi<'Candlestick'> | undefined
  let volumeSeries: ISeriesApi<'Histogram'> | undefined
  let vwapSeries: ISeriesApi<'Line'> | undefined
  let maSeries: ISeriesApi<'Line'> | undefined
  let applyingExternalRange = false
  let previousRowCount = 0
  let previousLastOpenTime: number | undefined
  // Mirrors `props.onLoadEarlier` for the imperative Lightweight Charts
  // callback below, which isn't a tracked scope: reading `props.x`
  // directly there would (rightly) trip the solid/reactivity lint rule,
  // since Solid can't see that a plain closure re-reads it live.
  let onLoadEarlier: (() => void) | undefined
  createEffect(() => {
    onLoadEarlier = props.onLoadEarlier
  })

  onMount(() => {
    if (!container) return
    const gestures = trackUserGestures(container)

    chart = createChart(container, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: cssToken('--color-surface') },
        textColor: cssToken('--color-fg'),
      },
      grid: {
        vertLines: { color: cssToken('--color-border') },
        horzLines: { color: cssToken('--color-border') },
      },
      timeScale: { timeVisible: true, secondsVisible: false },
    })

    const upColor = cssToken('--color-up')
    const downColor = cssToken('--color-down')

    candleSeries = chart.addSeries(CandlestickSeries, {
      upColor,
      downColor,
      borderVisible: false,
      wickUpColor: upColor,
      wickDownColor: downColor,
    })

    volumeSeries = chart.addSeries(HistogramSeries, {
      priceFormat: { type: 'volume' },
      priceScaleId: 'volume',
    })
    volumeSeries.priceScale().applyOptions({ scaleMargins: { top: 0.8, bottom: 0 } })

    vwapSeries = chart.addSeries(LineSeries, {
      color: cssToken('--color-accent'),
      lineWidth: 2,
      visible: props.showVwap ?? false,
      priceLineVisible: false,
      lastValueVisible: false,
    })

    maSeries = chart.addSeries(LineSeries, {
      color: cssToken('--color-fg-muted'),
      lineWidth: 1,
      visible: props.showMa ?? false,
      priceLineVisible: false,
      lastValueVisible: false,
    })

    // FR-4.2: publish this chart's visible range so every other
    // chart/panel follows along, unless the range change was *us*
    // applying someone else's published range a moment ago.
    chart.timeScale().subscribeVisibleTimeRangeChange((range) => {
      if (range === null || applyingExternalRange || !gestures.wasRecentUserGesture()) return
      publishVisibleRange(chartId, { from: range.from as number, to: range.to as number })
    })

    // FR-3.5: ask for more history once scrolled near the start of what's
    // loaded. `barsInfo`/logical range is in bar-index space, not time, so
    // this doesn't need to know the actual timestamps at all.
    chart.timeScale().subscribeVisibleLogicalRangeChange((range: LogicalRange | null) => {
      if (range === null) return
      if (range.from < LOAD_EARLIER_THRESHOLD_BARS) {
        onLoadEarlier?.()
      }
    })

    onCleanup(() => {
      gestures.stop()
      chart?.remove()
    })
  })

  // FR-4.2: apply a range published by another chart/panel, unless we're
  // the one who published it (checked via `origin`, not a value compare —
  // two different charts could legitimately end up with the same range).
  createEffect(() => {
    const range = visibleRange()
    if (range === null || origin() === chartId || chart === undefined) return
    applyingExternalRange = true
    chart.timeScale().setVisibleRange({ from: range.from as UTCTimestamp, to: range.to as UTCTimestamp })
    applyingExternalRange = false
  })

  createEffect(() => {
    vwapSeries?.applyOptions({ visible: props.showVwap ?? false })
  })

  createEffect(() => {
    maSeries?.applyOptions({ visible: props.showMa ?? false })
  })

  createEffect(() => {
    // `unwrap`: `props.data` (ultimately `@tanstack/solid-query`'s `data`)
    // is backed by a Solid store proxy. Iterating it as a store — reading
    // 5-8 fields per row across 50k+ rows — routes every field read
    // through the store's reactive-tracking proxy trap; profiled at
    // ~450ms of pure proxy overhead for a 50k-row set, the entire gap
    // between this and the ~90ms actually spent in our own code (NFR-1.3
    // needs the whole thing under 200ms). `unwrap` drops back to the
    // plain, non-reactive array/objects for this one-shot bulk read —
    // safe because nothing here needs to track individual field changes,
    // only the top-level `props.data` identity (already tracked below).
    const rows = unwrap(props.data)
    if (candleSeries === undefined || volumeSeries === undefined || chart === undefined) return

    const upColor = cssToken('--color-up')
    const downColor = cssToken('--color-down')

    // FR-3.5: distinguish "more history was prepended to the same
    // series" (same last bar, more rows added at the front) from "a
    // fresh symbol/interval/range load" (different last bar, or the very
    // first load). Only the former should preserve the user's current
    // viewport — the latter should fit-to-content like any new chart.
    const isHistoryPrepend =
      previousRowCount > 0 && rows.length > previousRowCount && rows.at(-1)?.openTime === previousLastOpenTime

    const priorLogicalRange = isHistoryPrepend ? chart.timeScale().getVisibleLogicalRange() : null
    const barCountDelta = rows.length - previousRowCount

    // NFR-1.3 measures the candles' own first paint (<200ms for 50k bars)
    // — the MA(20) overlay is a secondary layer, computed and applied one
    // frame later so it can't push the primary content over budget. Was
    // inline here originally; profiled at adding ~50-70ms on its own
    // (a third full-size `setData()` call), enough to blow the budget.
    performance.mark('price-chart:set-data:start')
    candleSeries.setData(rows.map(toCandlestickPoint))
    volumeSeries.setData(rows.map((row) => toVolumePoint(row, upColor, downColor)))

    if (priorLogicalRange !== null) {
      chart.timeScale().setVisibleLogicalRange({
        from: priorLogicalRange.from + barCountDelta,
        to: priorLogicalRange.to + barCountDelta,
      })
    } else {
      chart.timeScale().fitContent()
    }
    performance.mark('price-chart:set-data:end')

    previousRowCount = rows.length
    previousLastOpenTime = rows.at(-1)?.openTime

    if (import.meta.env.DEV) {
      const measure = performance.measure(
        'price-chart:set-data',
        'price-chart:set-data:start',
        'price-chart:set-data:end',
      )
      // NFR-1.3: first render of 50k candles must land under 200ms.
      console.debug(`[price-chart] setData(${rows.length} rows): ${measure.duration.toFixed(1)}ms`)
    }

    requestAnimationFrame(() => {
      maSeries?.setData(toMaSeries(rows))
    })
  })

  // FR-3.3/Этап 3: the live path. `update()` both revises the in-progress
  // last bar (repeated calls with the same `time`) and appends a new one
  // once the interval rolls over (a later `time`) — either way, no
  // re-render of the rest of the series. Guarded against firing before
  // the first historical `setData()` (a live tick can technically arrive
  // before the REST load resolves) and against an out-of-order tick
  // (older than the bar the chart already shows) — Lightweight Charts
  // throws on a `time` earlier than what's already in the series.
  createEffect(() => {
    const live = props.liveKline
    if (live === null || live === undefined) return
    if (candleSeries === undefined || volumeSeries === undefined) return
    if (previousRowCount === 0) return
    if (previousLastOpenTime !== undefined && live.openTime < previousLastOpenTime) return

    const upColor = cssToken('--color-up')
    const downColor = cssToken('--color-down')
    candleSeries.update(toCandlestickPoint(live))
    volumeSeries.update(toVolumePoint(live, upColor, downColor))
    previousLastOpenTime = live.openTime
  })

  createEffect(() => {
    if (vwapSeries === undefined) return
    const points = (props.vwapData ?? [])
      .map(toVwapPoint)
      .filter((p): p is { time: UTCTimestamp; value: number } => p.value !== null)
    vwapSeries.setData(points)
  })

  return <div ref={container} data-testid="price-chart" class="h-full w-full" />
}
