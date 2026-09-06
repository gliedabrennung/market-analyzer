import { createQuery } from '@tanstack/solid-query'
import { IndicatorPanel } from '../charts/indicator-panel'
import { cssToken } from '../charts/theme'
import { fetchAnomalies } from '../api/endpoints'
import type { Interval } from '../api/types'

export interface VolumeZScorePanelProps {
  symbol: string
  interval: Interval
}

const WINDOW = 100
/** FR-3.4's own default. `/analytics/{symbol}/anomalies` only ever
 * returns bars whose z-score *exceeds* `threshold` (the backend rejects
 * a non-positive one outright) — there's no endpoint for "the z-score of
 * every bar", so this panel shows the same flagged spikes FR-5.1's
 * anomalies table will (Этап 4), just as discrete markers over time
 * (`style: 'points'`) rather than a fabricated continuous line. */
const THRESHOLD = 3.0

/** FR-4.1: volume z-score panel — detected volume anomalies over time. */
export function VolumeZScorePanel(props: VolumeZScorePanelProps) {
  const query = createQuery(() => ({
    queryKey: ['volume-zscore', props.symbol, props.interval],
    queryFn: ({ signal }) =>
      fetchAnomalies(
        { symbol: props.symbol, interval: props.interval, window: WINDOW, threshold: THRESHOLD },
        { signal },
      ),
  }))

  return (
    <IndicatorPanel
      title="Volume Z-Score (anomalies)"
      times={(query.data ?? []).map((a) => a.openTime / 1000)}
      series={[
        {
          label: 'z-score',
          color: cssToken('--color-accent'),
          values: (query.data ?? []).map((a) => a.zScore),
          style: 'points',
        },
      ]}
    />
  )
}
