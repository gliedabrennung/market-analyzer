import { createMemo } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { IndicatorPanel } from '../charts/indicator-panel'
import { cssToken } from '../charts/theme'
import { fetchOfi } from '../api/endpoints'

export interface OfiPanelProps {
  symbol: string
}

const BUCKET_SECONDS = 60
const LOOKBACK_HOURS = 24
const MS_PER_HOUR = 60 * 60 * 1000

/** FR-4.1: Order Flow Imbalance panel. Unlike vwap/volatility/anomalies,
 * `/analytics/{symbol}/ofi` *does* take a `from`/`to` range (it reads the
 * `trades` dataset, not klines) — OFI is a microstructure indicator, a
 * fixed lookback window makes more sense here than mirroring the main
 * chart's (potentially 30-day) range. */
export function OfiPanel(props: OfiPanelProps) {
  const range = createMemo(() => {
    const to = new Date()
    const from = new Date(to.getTime() - LOOKBACK_HOURS * MS_PER_HOUR)
    return { from: from.toISOString(), to: to.toISOString() }
  })

  const query = createQuery(() => ({
    queryKey: ['ofi', props.symbol, range().from, range().to],
    queryFn: ({ signal }) =>
      fetchOfi(
        { symbol: props.symbol, bucketSeconds: BUCKET_SECONDS, from: range().from, to: range().to },
        { signal },
      ),
  }))

  return (
    <IndicatorPanel
      title={`OFI (последние ${LOOKBACK_HOURS}ч)`}
      times={(query.data ?? []).map((b) => b.bucket / 1000)}
      series={[
        { label: 'ofi', color: cssToken('--color-accent'), values: (query.data ?? []).map((b) => b.ofi) },
      ]}
    />
  )
}

// `solid-js`'s `lazy()` (app.tsx: off by default, state/settings.ts) needs
// a default export.
export default OfiPanel
