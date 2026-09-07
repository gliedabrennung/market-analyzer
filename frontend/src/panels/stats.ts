import type { OhlcvRow } from '../api/types'

export interface WindowStats {
  changePercent: number | null
  volume: number
}

const MS_PER_DAY = 24 * 60 * 60 * 1000

/** FR-8.2's "изменение за 24ч"/"объём": computed from whatever candles are
 * already loaded (`rows`, ascending by `openTime`) rather than a
 * dedicated endpoint — no backend endpoint returns this directly, and the
 * default 30-day load already covers the last 24h in every normal case.
 * `changePercent` is `null` when there's no bar at/after the 24h cutoff
 * yet (fresh symbol, or `currentPrice` unknown) — not `0`, which would
 * misleadingly claim "unchanged". */
export function compute24hStats(
  rows: readonly OhlcvRow[],
  currentPrice: number | null,
  nowMs: number,
): WindowStats {
  const cutoff = nowMs - MS_PER_DAY
  const inWindow = rows.filter((r) => r.openTime >= cutoff)
  const volume = inWindow.reduce((sum, r) => sum + r.volume, 0)

  const baseline = inWindow[0]
  if (baseline === undefined || currentPrice === null || baseline.close === 0) {
    return { changePercent: null, volume }
  }
  return { changePercent: ((currentPrice - baseline.close) / baseline.close) * 100, volume }
}
