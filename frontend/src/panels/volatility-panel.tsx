import { createQuery } from '@tanstack/solid-query'
import { IndicatorPanel } from '../charts/indicator-panel'
import { themedCssToken } from '../charts/theme'
import { fetchVolatility } from '../api/endpoints'
import type { Interval } from '../api/types'

export interface VolatilityPanelProps {
  symbol: string
  interval: Interval
}

const WINDOW = 20

/** FR-4.1: realized volatility panel. */
export function VolatilityPanel(props: VolatilityPanelProps) {
  const query = createQuery(() => ({
    queryKey: ['volatility', props.symbol, props.interval],
    queryFn: ({ signal }) =>
      fetchVolatility({ symbol: props.symbol, interval: props.interval, window: WINDOW }, { signal }),
  }))
  const seriesColor = themedCssToken('--color-accent')

  return (
    <IndicatorPanel
      title="Realized Volatility"
      times={(query.data ?? []).map((p) => p.openTime / 1000)}
      series={[
        {
          label: 'volatility',
          color: seriesColor(),
          values: (query.data ?? []).map((p) => p.realizedVolatility),
        },
      ]}
    />
  )
}
