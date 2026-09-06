import { describe, expect, it } from 'vitest'
import {
  Bool,
  Float64,
  Int32,
  Table,
  TimestampMillisecond,
  tableToIPC,
  Utf8,
  vectorFromArray,
} from 'apache-arrow'
import {
  parseAnomaliesArrow,
  parseOfiArrow,
  parseOhlcvArrow,
  parseVolatilityArrow,
  parseVwapArrow,
} from './arrow'

/** Builds the same column layout `ma_api::arrow_ipc::ToRecordBatch for
 * OhlcvRow` produces (crates/api/src/arrow_ipc.rs) — schema was verified
 * against the real backend's bytes during development. */
function buildIpc(
  rows: {
    openTime: number
    open: string
  }[],
): Uint8Array {
  const table = new Table({
    open_time: vectorFromArray(
      rows.map((r) => r.openTime),
      new TimestampMillisecond('UTC'),
    ),
    close_time: vectorFromArray(
      rows.map((r) => r.openTime + 59_999),
      new TimestampMillisecond('UTC'),
    ),
    open: vectorFromArray(
      rows.map((r) => r.open),
      new Utf8(),
    ),
    high: vectorFromArray(
      rows.map(() => '101.5'),
      new Utf8(),
    ),
    low: vectorFromArray(
      rows.map(() => '99.5'),
      new Utf8(),
    ),
    close: vectorFromArray(
      rows.map(() => '100.5'),
      new Utf8(),
    ),
    volume: vectorFromArray(
      rows.map(() => '42.12345678'),
      new Utf8(),
    ),
    quote_volume: vectorFromArray(
      rows.map(() => '4234.5'),
      new Utf8(),
    ),
    trades_count: vectorFromArray(
      rows.map(() => 7),
      new Int32(),
    ),
    is_closed: vectorFromArray(
      rows.map(() => true),
      new Bool(),
    ),
  })
  return tableToIPC(table, 'stream')
}

describe('parseOhlcvArrow', () => {
  it('round-trips timestamps as epoch-ms numbers and decimals as parsed floats', () => {
    const bytes = buildIpc([{ openTime: 1_700_000_000_000, open: '100.00000001' }])
    const rows = parseOhlcvArrow(bytes)

    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({
      openTime: 1_700_000_000_000,
      closeTime: 1_700_000_059_999,
      open: 100.00000001,
      high: 101.5,
      low: 99.5,
      close: 100.5,
      volume: 42.12345678,
      tradesCount: 7,
      isClosed: true,
    })
  })

  it('returns an empty array for an empty table', () => {
    const bytes = buildIpc([])
    const rows = parseOhlcvArrow(bytes)
    expect(rows).toEqual([])
  })
})

describe('parseVwapArrow', () => {
  it('parses a null vwap (zero-volume window) as null, not NaN', () => {
    const table = new Table({
      open_time: vectorFromArray([1_700_000_000_000, 1_700_000_060_000], new TimestampMillisecond('UTC')),
      close: vectorFromArray(['100.5', '101.5'], new Utf8()),
      vwap: vectorFromArray(['100.25', null], new Utf8()),
    })
    const rows = parseVwapArrow(tableToIPC(table, 'stream'))

    expect(rows).toEqual([
      { openTime: 1_700_000_000_000, close: 100.5, vwap: 100.25 },
      { openTime: 1_700_000_060_000, close: 101.5, vwap: null },
    ])
  })
})

describe('parseVolatilityArrow', () => {
  it('parses a null realized_volatility as null', () => {
    const table = new Table({
      open_time: vectorFromArray([1_700_000_000_000], new TimestampMillisecond('UTC')),
      realized_volatility: vectorFromArray([null], new Float64()),
    })
    const rows = parseVolatilityArrow(tableToIPC(table, 'stream'))
    expect(rows).toEqual([{ openTime: 1_700_000_000_000, realizedVolatility: null }])
  })
})

describe('parseAnomaliesArrow', () => {
  it('parses volume as a float and z_score as a number', () => {
    const table = new Table({
      open_time: vectorFromArray([1_700_000_000_000], new TimestampMillisecond('UTC')),
      volume: vectorFromArray(['123.45'], new Utf8()),
      z_score: vectorFromArray([7.07], new Float64()),
    })
    const rows = parseAnomaliesArrow(tableToIPC(table, 'stream'))
    expect(rows).toEqual([{ openTime: 1_700_000_000_000, volume: 123.45, zScore: 7.07 }])
  })
})

describe('parseOfiArrow', () => {
  it('parses buy/sell volume as floats and ofi as a ratio', () => {
    const table = new Table({
      bucket: vectorFromArray([1_700_000_000_000], new TimestampMillisecond('UTC')),
      buy_volume: vectorFromArray(['5.00000000'], new Utf8()),
      sell_volume: vectorFromArray(['1.00000000'], new Utf8()),
      ofi: vectorFromArray([4 / 6], new Float64()),
    })
    const rows = parseOfiArrow(tableToIPC(table, 'stream'))
    expect(rows).toEqual([{ bucket: 1_700_000_000_000, buyVolume: 5, sellVolume: 1, ofi: 4 / 6 }])
  })
})
