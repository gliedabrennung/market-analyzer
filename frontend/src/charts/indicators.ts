import type { OhlcvRow } from '../api/types'

/** Simple moving average of `close` over the trailing `window` bars
 * (inclusive of the current one). `null` before enough bars exist yet —
 * same "not enough history" convention the backend's own rolling-window
 * analytics use (vwap, volatility). A client-side indicator: no backend
 * endpoint computes a plain MA, and it needs nothing beyond the candles
 * already on screen. */
export function simpleMovingAverage(rows: readonly OhlcvRow[], window: number): (number | null)[] {
  if (window <= 0) {
    throw new Error('window must be positive')
  }

  const result: (number | null)[] = new Array(rows.length)
  let sum = 0
  for (let i = 0; i < rows.length; i++) {
    sum += rows[i]!.close
    if (i >= window) {
      sum -= rows[i - window]!.close
    }
    result[i] = i >= window - 1 ? sum / window : null
  }
  return result
}
