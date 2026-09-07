import type { OhlcvRow } from '../api/types'

export interface WindowStats {
  changePercent: number | null
  volume: number
}

const MS_PER_DAY = 24 * 60 * 60 * 1000

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
