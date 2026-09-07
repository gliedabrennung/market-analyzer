import { describe, expect, it } from 'vitest'
import { compute24hStats } from './stats'
import type { OhlcvRow } from '../api/types'

function candle(openTime: number, close: number, volume = 1): OhlcvRow {
  return {
    openTime,
    closeTime: openTime + 59_999,
    open: close,
    high: close,
    low: close,
    close,
    volume,
    quoteVolume: 0,
    tradesCount: 0,
    isClosed: true,
  }
}

const DAY = 24 * 60 * 60 * 1000
const NOW = 10 * DAY

describe('compute24hStats', () => {
  it('computes % change from the first bar at/after the 24h cutoff', () => {
    const rows = [candle(NOW - 2 * DAY, 100), candle(NOW - DAY, 200), candle(NOW - DAY / 2, 250)]
    const stats = compute24hStats(rows, 300, NOW)

    expect(stats.changePercent).toBe(50)
  })

  it('sums volume only for bars within the last 24h', () => {
    const rows = [candle(NOW - 2 * DAY, 100, 999), candle(NOW - DAY, 100, 5), candle(NOW - DAY / 2, 100, 3)]
    const stats = compute24hStats(rows, 100, NOW)
    expect(stats.volume).toBe(8)
  })

  it('returns null change (not 0) when nothing falls in the window yet', () => {
    const rows = [candle(NOW - 5 * DAY, 100)]
    const stats = compute24hStats(rows, 100, NOW)
    expect(stats.changePercent).toBeNull()
  })

  it('returns null change when currentPrice is unknown', () => {
    const rows = [candle(NOW - DAY, 100)]
    const stats = compute24hStats(rows, null, NOW)
    expect(stats.changePercent).toBeNull()
  })
})
