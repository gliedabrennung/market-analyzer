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

export interface FixtureCandle {
  openTime: number
  open: string
  high: string
  low: string
  close: string
  volume: string
}

/** Builds an Arrow IPC stream byte-for-byte in the same shape
 * `ma_api::arrow_ipc::ToRecordBatch for OhlcvRow` produces (see
 * `crates/api/src/arrow_ipc.rs`) — verified against the real backend's
 * bytes during development, see the session notes. Used to mock
 * `/ohlcv/*` in e2e tests without needing a live backend. */
export function buildOhlcvArrowIpc(candles: FixtureCandle[]): Uint8Array {
  const table = new Table({
    open_time: vectorFromArray(
      candles.map((c) => c.openTime),
      new TimestampMillisecond('UTC'),
    ),
    close_time: vectorFromArray(
      candles.map((c) => c.openTime + 59_999),
      new TimestampMillisecond('UTC'),
    ),
    open: vectorFromArray(
      candles.map((c) => c.open),
      new Utf8(),
    ),
    high: vectorFromArray(
      candles.map((c) => c.high),
      new Utf8(),
    ),
    low: vectorFromArray(
      candles.map((c) => c.low),
      new Utf8(),
    ),
    close: vectorFromArray(
      candles.map((c) => c.close),
      new Utf8(),
    ),
    volume: vectorFromArray(
      candles.map((c) => c.volume),
      new Utf8(),
    ),
    quote_volume: vectorFromArray(
      candles.map((c) => c.volume),
      new Utf8(),
    ),
    trades_count: vectorFromArray(
      candles.map(() => 10),
      new Int32(),
    ),
    is_closed: vectorFromArray(
      candles.map(() => true),
      new Bool(),
    ),
  })
  return tableToIPC(table, 'stream')
}

export interface FixtureAnomaly {
  openTime: number
  volume: string
  zScore: number
}

/** Same reasoning as `buildOhlcvArrowIpc` — matches
 * `ma_api::arrow_ipc::ToRecordBatch for VolumeAnomaly` byte-for-byte. */
export function buildAnomaliesArrowIpc(anomalies: FixtureAnomaly[]): Uint8Array {
  const table = new Table({
    open_time: vectorFromArray(
      anomalies.map((a) => a.openTime),
      new TimestampMillisecond('UTC'),
    ),
    volume: vectorFromArray(
      anomalies.map((a) => a.volume),
      new Utf8(),
    ),
    z_score: vectorFromArray(
      anomalies.map((a) => a.zScore),
      new Float64(),
    ),
  })
  return tableToIPC(table, 'stream')
}

export function makeFixtureCandles(count: number): FixtureCandle[] {
  const start = Date.UTC(2026, 7, 1)
  const candles: FixtureCandle[] = []
  let price = 50_000
  for (let i = 0; i < count; i++) {
    const open = price
    const close = price + (i % 7) - 3
    price = close
    candles.push({
      openTime: start + i * 60_000,
      open: open.toFixed(8),
      high: (Math.max(open, close) + 5).toFixed(8),
      low: (Math.min(open, close) - 5).toFixed(8),
      close: close.toFixed(8),
      volume: (10 + (i % 5)).toFixed(8),
    })
  }
  return candles
}
