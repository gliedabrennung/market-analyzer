import { describe, expect, it } from 'vitest'
import { buildOhlcvCsv } from './csv'
import type { OhlcvRow } from '../api/types'

function candle(
  openTime: number,
  open: number,
  high: number,
  low: number,
  close: number,
  volume: number,
): OhlcvRow {
  return {
    openTime,
    closeTime: openTime + 59_999,
    open,
    high,
    low,
    close,
    volume,
    quoteVolume: 0,
    tradesCount: 0,
    isClosed: true,
  }
}

describe('buildOhlcvCsv', () => {
  it('writes a header row and one line per candle, oldest first', () => {
    const rows = [
      candle(Date.UTC(2026, 0, 1), 100, 110, 90, 105, 12.5),
      candle(Date.UTC(2026, 0, 1, 0, 1), 105, 108, 104, 106, 3),
    ]
    const csv = buildOhlcvCsv(rows)
    const lines = csv.split('\n')
    expect(lines[0]).toBe('open_time,open,high,low,close,volume')
    expect(lines).toHaveLength(3)
    expect(lines[1]).toBe('2026-01-01T00:00:00.000Z,100,110,90,105,12.5')
    expect(lines[2]).toBe('2026-01-01T00:01:00.000Z,105,108,104,106,3')
  })

  it('returns just the header for an empty input', () => {
    expect(buildOhlcvCsv([])).toBe('open_time,open,high,low,close,volume')
  })
})
