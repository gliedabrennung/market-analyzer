import { describe, expect, it } from 'vitest'
import { Bool, Int32, Table, TimestampMillisecond, tableToIPC, Utf8, vectorFromArray } from 'apache-arrow'
import { parseOhlcvArrow } from './arrow'

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
