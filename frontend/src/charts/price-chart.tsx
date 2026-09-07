import { createEffect, on, onCleanup, onMount } from 'solid-js'
import { unwrap } from 'solid-js/store'
import {
  CandlestickSeries,
  ColorType,
  HistogramSeries,
  LineSeries,
  createChart,
  createSeriesMarkers,
  type IChartApi,
  type ISeriesApi,
  type ISeriesMarkersPluginApi,
  type LogicalRange,
  type Time,
  type UTCTimestamp,
} from 'lightweight-charts'
import type { OhlcvRow, VwapPoint } from '../api/types'
import { simpleMovingAverage } from './indicators'
import { cssToken } from './theme'
import { origin, publishVisibleRange, trackUserGestures, visibleRange } from './sync'
import { colorblindPalette, theme } from '../state/theme'

const MA_WINDOW = 20

const LOAD_EARLIER_THRESHOLD_BARS = 20

export interface PriceChartProps {
  data: OhlcvRow[]
  vwapData?: VwapPoint[]
  showVwap?: boolean
  showMa?: boolean

  liveKline?: OhlcvRow | null

  onLoadEarlier?: () => void

  jumpTarget?: { time: number; nonce: number } | null

  resetZoomNonce?: number
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

function toMaSeries(rows: OhlcvRow[]) {
  const ma = simpleMovingAverage(rows, MA_WINDOW)
  return rows
    .map((row, i) => ({ time: (row.openTime / 1000) as UTCTimestamp, value: ma[i] }))
    .filter((point): point is { time: UTCTimestamp; value: number } => point.value !== null)
}

export function PriceChart(props: PriceChartProps) {
  const chartId = Symbol('price-chart')
  let container: HTMLDivElement | undefined
  let chart: IChartApi | undefined
  let candleSeries: ISeriesApi<'Candlestick'> | undefined
  let volumeSeries: ISeriesApi<'Histogram'> | undefined
  let vwapSeries: ISeriesApi<'Line'> | undefined
  let maSeries: ISeriesApi<'Line'> | undefined
  let markers: ISeriesMarkersPluginApi<Time> | undefined
  let lastMarkerTime: UTCTimestamp | undefined
  let applyingExternalRange = false
  let previousRowCount = 0
  let previousLastOpenTime: number | undefined

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

    markers = createSeriesMarkers(candleSeries, [])

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

    chart.timeScale().subscribeVisibleTimeRangeChange((range) => {
      if (range === null || applyingExternalRange || !gestures.wasRecentUserGesture()) return
      publishVisibleRange(chartId, { from: range.from as number, to: range.to as number })
    })

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
    const rows = unwrap(props.data)
    if (candleSeries === undefined || volumeSeries === undefined || chart === undefined) return

    const upColor = cssToken('--color-up')
    const downColor = cssToken('--color-down')

    const isHistoryPrepend =
      previousRowCount > 0 && rows.length > previousRowCount && rows.at(-1)?.openTime === previousLastOpenTime

    const priorLogicalRange = isHistoryPrepend ? chart.timeScale().getVisibleLogicalRange() : null
    const barCountDelta = rows.length - previousRowCount

    performance.mark('price-chart:set-data:start')
    candleSeries.setData(rows.map(toCandlestickPoint))
    volumeSeries.setData(rows.map((row) => toVolumePoint(row, upColor, downColor)))

    if (priorLogicalRange !== null) {
      chart.timeScale().setVisibleLogicalRange({
        from: priorLogicalRange.from + barCountDelta,
        to: priorLogicalRange.to + barCountDelta,
      })
    } else {
      lastMarkerTime = undefined
      markers?.setMarkers([])
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

      console.debug(`[price-chart] setData(${rows.length} rows): ${measure.duration.toFixed(1)}ms`)
    }

    requestAnimationFrame(() => {
      maSeries?.setData(toMaSeries(rows))
    })
  })

  createEffect(() => {
    const live = props.liveKline
    if (live === null || live === undefined) return
    if (candleSeries === undefined || volumeSeries === undefined) return
    if (previousRowCount === 0) return
    if (previousLastOpenTime !== undefined && live.openTime < previousLastOpenTime) return

    const upColor = cssToken('--color-up')
    const downColor = cssToken('--color-down')
    try {
      candleSeries.update(toCandlestickPoint(live))
      volumeSeries.update(toVolumePoint(live, upColor, downColor))
      previousLastOpenTime = live.openTime
    } catch (error) {
      console.error('[price-chart] dropped a live update the chart rejected', error)
    }
  })

  createEffect(() => {
    if (vwapSeries === undefined) return
    const points = (props.vwapData ?? [])
      .map(toVwapPoint)
      .filter((p): p is { time: UTCTimestamp; value: number } => p.value !== null)
    vwapSeries.setData(points)
  })

  createEffect(() => {
    const target = props.jumpTarget
    if (target == null || chart === undefined || markers === undefined) return

    const timeSec = Math.floor(target.time / 1000) as UTCTimestamp
    lastMarkerTime = timeSec
    markers.setMarkers([
      { time: timeSec, position: 'aboveBar', shape: 'arrowDown', color: cssToken('--color-down'), size: 1.5 },
    ])

    const currentRange = chart.timeScale().getVisibleRange()
    const span = currentRange !== null ? (currentRange.to as number) - (currentRange.from as number) : 3600
    const half = span / 2
    const newRange = {
      from: ((timeSec as number) - half) as UTCTimestamp,
      to: ((timeSec as number) + half) as UTCTimestamp,
    }
    applyingExternalRange = true
    chart.timeScale().setVisibleRange(newRange)
    applyingExternalRange = false
    publishVisibleRange(chartId, { from: newRange.from as number, to: newRange.to as number })

    container?.setAttribute('data-last-jump-nonce', String(target.nonce))
  })

  createEffect(() => {
    if (props.resetZoomNonce === undefined || chart === undefined) return
    chart.timeScale().fitContent()
  })

  createEffect(
    on(
      [theme, colorblindPalette],
      () => {
        if (chart === undefined || candleSeries === undefined || volumeSeries === undefined) return

        const upColor = cssToken('--color-up')
        const downColor = cssToken('--color-down')

        chart.applyOptions({
          layout: {
            background: { type: ColorType.Solid, color: cssToken('--color-surface') },
            textColor: cssToken('--color-fg'),
          },
          grid: {
            vertLines: { color: cssToken('--color-border') },
            horzLines: { color: cssToken('--color-border') },
          },
        })
        candleSeries.applyOptions({ upColor, downColor, wickUpColor: upColor, wickDownColor: downColor })
        volumeSeries.setData(unwrap(props.data).map((row) => toVolumePoint(row, upColor, downColor)))
        vwapSeries?.applyOptions({ color: cssToken('--color-accent') })
        maSeries?.applyOptions({ color: cssToken('--color-fg-muted') })

        if (lastMarkerTime !== undefined) {
          markers?.setMarkers([
            { time: lastMarkerTime, position: 'aboveBar', shape: 'arrowDown', color: downColor, size: 1.5 },
          ])
        }
      },
      { defer: true },
    ),
  )

  return <div ref={container} data-testid="price-chart" class="h-full w-full" />
}
