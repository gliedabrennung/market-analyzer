import { createMemo } from 'solid-js'
import { colorblindPalette, theme } from '../state/theme'

/** Reads a design token (`tokens.css`, DR-2) as a resolved color string,
 * for the chart libraries (Lightweight Charts, uPlot) that need real CSS
 * color values up front rather than a stylesheet class. */
export function cssToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

/** Like `cssToken`, but tracked: re-reads whenever the theme or
 * colorblind palette changes. For a chart-library color *prop* (e.g.
 * `IndicatorSeries.color`, passed down into `IndicatorPanel` and baked
 * into uPlot at construction) — `cssToken` alone only re-reads when
 * whatever expression called it happens to re-run for some *other*
 * reason (e.g. `query.data` changing), which silently leaves the color
 * stale until the next unrelated re-render after a theme flip. */
export function themedCssToken(name: string): () => string {
  const value = createMemo(() => {
    theme()
    colorblindPalette()
    return cssToken(name)
  })
  return value
}
