import { createQuery } from '@tanstack/solid-query'
import { IndicatorPanel } from '../charts/indicator-panel'
import { themedCssToken } from '../charts/theme'
import { fetchAnomalies } from '../api/endpoints'
import type { Interval } from '../api/types'

export interface VolumeZScorePanelProps {
  symbol: string
  interval: Interval
}

const WINDOW = 100

const THRESHOLD = 3.0

export function VolumeZScorePanel(props: VolumeZScorePanelProps) {
  const query = createQuery(() => ({
    queryKey: ['volume-zscore', props.symbol, props.interval],
    queryFn: ({ signal }) =>
      fetchAnomalies(
        { symbol: props.symbol, interval: props.interval, window: WINDOW, threshold: THRESHOLD },
        { signal },
      ),
  }))
  const seriesColor = themedCssToken('--color-accent')

  return (
    <IndicatorPanel
      title="Volume Z-Score (anomalies)"
      times={(query.data ?? []).map((a) => a.openTime / 1000)}
      series={[
        {
          label: 'z-score',
          color: seriesColor(),
          values: (query.data ?? []).map((a) => a.zScore),
          style: 'points',
        },
      ]}
    />
  )
}
