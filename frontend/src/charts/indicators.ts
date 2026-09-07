import type { OhlcvRow } from '../api/types'

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
