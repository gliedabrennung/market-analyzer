import { createEffect, onCleanup, onMount } from 'solid-js'
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'
import { cssToken } from './theme'
import { origin, publishVisibleRange, trackUserGestures, visibleRange } from './sync'

export interface IndicatorSeries {
  label: string
  color: string
  values: (number | null)[]

  style?: 'line' | 'points'
}

export interface IndicatorPanelProps {
  title: string

  times: number[]
  series: IndicatorSeries[]
  height?: number
}

const DEFAULT_HEIGHT = 110

export function IndicatorPanel(props: IndicatorPanelProps) {
  const panelId = Symbol('indicator-panel')
  let container: HTMLDivElement | undefined
  let plot: uPlot | undefined
  let applyingExternalRange = false
  let wasRecentUserGesture: () => boolean = () => false

  // eslint-disable-next-line solid/reactivity
  let previousColors = props.series.map((s) => s.color)

  function toAlignedData(): uPlot.AlignedData {
    return [props.times, ...props.series.map((s) => s.values)]
  }

  function buildOptions(width: number, height: number): uPlot.Options {
    return {
      width,
      height,
      padding: [8, 8, 0, 0],
      cursor: { drag: { x: true, y: false } },
      legend: { show: false },
      series: [
        {},
        ...props.series.map((s) =>
          s.style === 'points'
            ? { label: s.label, stroke: s.color, width: 0, points: { show: true, size: 5, fill: s.color } }
            : { label: s.label, stroke: s.color, width: 1.5, points: { show: false } },
        ),
      ],
      axes: [
        { stroke: cssToken('--color-fg-muted'), grid: { stroke: cssToken('--color-border') } },
        { stroke: cssToken('--color-fg-muted'), grid: { stroke: cssToken('--color-border') } },
      ],
      scales: { x: { time: true } },
      hooks: {
        setScale: [
          (u, key) => {
            if (key !== 'x' || applyingExternalRange || !wasRecentUserGesture()) return
            const xScale = u.scales.x
            if (xScale?.min === undefined || xScale.max === undefined) return
            publishVisibleRange(panelId, { from: xScale.min, to: xScale.max })
          },
        ],
      },
    }
  }

  onMount(() => {
    if (!container) return
    const gestures = trackUserGestures(container)
    wasRecentUserGesture = gestures.wasRecentUserGesture

    const width = container.clientWidth
    plot = new uPlot(buildOptions(width, props.height ?? DEFAULT_HEIGHT), toAlignedData(), container)

    const resizeObserver = new ResizeObserver(() => {
      if (container === undefined || plot === undefined) return
      plot.setSize({ width: container.clientWidth, height: props.height ?? DEFAULT_HEIGHT })
    })
    resizeObserver.observe(container)

    onCleanup(() => {
      resizeObserver.disconnect()
      gestures.stop()
      plot?.destroy()
    })
  })

  createEffect(() => {
    const colors = props.series.map((s) => s.color)
    const colorsChanged =
      colors.length !== previousColors.length || colors.some((c, i) => c !== previousColors[i])
    previousColors = colors

    if (plot === undefined || container === undefined) return
    if (colorsChanged) {
      plot.destroy()
      plot = new uPlot(
        buildOptions(container.clientWidth, props.height ?? DEFAULT_HEIGHT),
        toAlignedData(),
        container,
      )
    } else {
      plot.setData(toAlignedData())
    }
  })

  createEffect(() => {
    const range = visibleRange()
    if (range === null || origin() === panelId || plot === undefined) return
    applyingExternalRange = true
    plot.setScale('x', { min: range.from, max: range.to })
    applyingExternalRange = false
  })

  return (
    <div class="rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] p-2">
      <div class="mb-1 text-xs text-[var(--color-fg-muted)]">{props.title}</div>
      <div ref={container} class="w-full" />
    </div>
  )
}
