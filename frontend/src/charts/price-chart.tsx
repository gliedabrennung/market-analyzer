import { createEffect, onCleanup, onMount } from 'solid-js'
import { unwrap } from 'solid-js/store'
import {
  CandlestickSeries,
  ColorType,
  HistogramSeries,
  createChart,
  type IChartApi,
  type ISeriesApi,
  type UTCTimestamp,
} from 'lightweight-charts'
import type { OhlcvRow } from '../api/types'

export interface PriceChartProps {
  data: OhlcvRow[]
}

function cssToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
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

/** FR-3.1/FR-3.2: candlestick + volume, panning/zoom/crosshair are
 * Lightweight Charts defaults. FR-3.3's incremental-update path
 * (`series.update()` for the live last bar) lands in Этап 3 — this
 * component only does the full-series `setData()` load path so far. */
export function PriceChart(props: PriceChartProps) {
  let container: HTMLDivElement | undefined
  let chart: IChartApi | undefined
  let candleSeries: ISeriesApi<'Candlestick'> | undefined
  let volumeSeries: ISeriesApi<'Histogram'> | undefined

  onMount(() => {
    if (!container) return

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

    onCleanup(() => chart?.remove())
  })

  // FR-3.3: only a symbol/interval/range change should reach this
  // effect and re-run setData(); a live incremental candle (Этап 3) will
  // update the series directly and skip this path entirely.
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
    if (candleSeries === undefined || volumeSeries === undefined) return

    const upColor = cssToken('--color-up')
    const downColor = cssToken('--color-down')

    performance.mark('price-chart:set-data:start')
    candleSeries.setData(rows.map(toCandlestickPoint))
    volumeSeries.setData(rows.map((row) => toVolumePoint(row, upColor, downColor)))
    chart?.timeScale().fitContent()
    performance.mark('price-chart:set-data:end')

    if (import.meta.env.DEV) {
      const measure = performance.measure(
        'price-chart:set-data',
        'price-chart:set-data:start',
        'price-chart:set-data:end',
      )
      // NFR-1.3: first render of 50k candles must land under 200ms.
      console.debug(`[price-chart] setData(${rows.length} rows): ${measure.duration.toFixed(1)}ms`)
    }
  })

  return <div ref={container} class="h-full w-full" />
}
