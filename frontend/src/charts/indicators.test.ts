import { describe, expect, it } from 'vitest'
import { simpleMovingAverage } from './indicators'
import type { OhlcvRow } from '../api/types'

function candle(close: number): OhlcvRow {
  return {
    openTime: 0,
    closeTime: 0,
    open: close,
    high: close,
    low: close,
    close,
    volume: 0,
    quoteVolume: 0,
    tradesCount: 0,
    isClosed: true,
  }
}

describe('simpleMovingAverage', () => {
  it('is null before the window fills, then the trailing average', () => {
    const rows = [1, 2, 3, 4, 5].map(candle)
    expect(simpleMovingAverage(rows, 3)).toEqual([null, null, 2, 3, 4])
  })

  it('a window of 1 is just the close itself', () => {
    const rows = [10, 20, 30].map(candle)
    expect(simpleMovingAverage(rows, 1)).toEqual([10, 20, 30])
  })

  it('rejects a non-positive window', () => {
    expect(() => simpleMovingAverage([candle(1)], 0)).toThrow()
  })
})
