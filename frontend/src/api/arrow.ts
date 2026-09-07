import { tableFromIPC, type Table } from 'apache-arrow'
import type { CorrelationPair, OfiBucket, OhlcvRow, VolatilityPoint, VolumeAnomaly, VwapPoint } from './types'

function column(table: Table, name: string) {
  const col = table.getChild(name)
  if (col === null) {
    throw new Error(`Arrow response is missing expected column '${name}'`)
  }
  return col
}

function nullableDecimal(value: unknown): number | null {
  return value === null ? null : Number.parseFloat(value as string)
}

export function parseOhlcvArrow(bytes: Uint8Array): OhlcvRow[] {
  const table = tableFromIPC(bytes)

  const openTime = column(table, 'open_time')
  const closeTime = column(table, 'close_time')
  const open = column(table, 'open')
  const high = column(table, 'high')
  const low = column(table, 'low')
  const close = column(table, 'close')
  const volume = column(table, 'volume')
  const quoteVolume = column(table, 'quote_volume')
  const tradesCount = column(table, 'trades_count')
  const isClosed = column(table, 'is_closed')

  const rows: OhlcvRow[] = new Array(table.numRows)
  for (let i = 0; i < table.numRows; i++) {
    rows[i] = {
      openTime: Number(openTime.get(i)),
      closeTime: Number(closeTime.get(i)),
      open: Number.parseFloat(open.get(i) as string),
      high: Number.parseFloat(high.get(i) as string),
      low: Number.parseFloat(low.get(i) as string),
      close: Number.parseFloat(close.get(i) as string),
      volume: Number.parseFloat(volume.get(i) as string),
      quoteVolume: Number.parseFloat(quoteVolume.get(i) as string),
      tradesCount: Number(tradesCount.get(i)),
      isClosed: Boolean(isClosed.get(i)),
    }
  }
  return rows
}

export function parseVwapArrow(bytes: Uint8Array): VwapPoint[] {
  const table = tableFromIPC(bytes)
  const openTime = column(table, 'open_time')
  const close = column(table, 'close')
  const vwap = column(table, 'vwap')

  const rows: VwapPoint[] = new Array(table.numRows)
  for (let i = 0; i < table.numRows; i++) {
    rows[i] = {
      openTime: Number(openTime.get(i)),
      close: Number.parseFloat(close.get(i) as string),
      vwap: nullableDecimal(vwap.get(i)),
    }
  }
  return rows
}

export function parseVolatilityArrow(bytes: Uint8Array): VolatilityPoint[] {
  const table = tableFromIPC(bytes)
  const openTime = column(table, 'open_time')
  const realizedVolatility = column(table, 'realized_volatility')

  const rows: VolatilityPoint[] = new Array(table.numRows)
  for (let i = 0; i < table.numRows; i++) {
    const value = realizedVolatility.get(i)
    rows[i] = {
      openTime: Number(openTime.get(i)),
      realizedVolatility: value === null ? null : Number(value),
    }
  }
  return rows
}

export function parseAnomaliesArrow(bytes: Uint8Array): VolumeAnomaly[] {
  const table = tableFromIPC(bytes)
  const openTime = column(table, 'open_time')
  const volume = column(table, 'volume')
  const zScore = column(table, 'z_score')

  const rows: VolumeAnomaly[] = new Array(table.numRows)
  for (let i = 0; i < table.numRows; i++) {
    rows[i] = {
      openTime: Number(openTime.get(i)),
      volume: Number.parseFloat(volume.get(i) as string),
      zScore: Number(zScore.get(i)),
    }
  }
  return rows
}

export function parseCorrelationArrow(bytes: Uint8Array): CorrelationPair[] {
  const table = tableFromIPC(bytes)
  const symbolA = column(table, 'symbol_a')
  const symbolB = column(table, 'symbol_b')
  const correlation = column(table, 'correlation')

  const rows: CorrelationPair[] = new Array(table.numRows)
  for (let i = 0; i < table.numRows; i++) {
    const value = correlation.get(i)
    rows[i] = {
      symbolA: symbolA.get(i) as string,
      symbolB: symbolB.get(i) as string,
      correlation: value === null ? null : Number(value),
    }
  }
  return rows
}

export function parseOfiArrow(bytes: Uint8Array): OfiBucket[] {
  const table = tableFromIPC(bytes)
  const bucket = column(table, 'bucket')
  const buyVolume = column(table, 'buy_volume')
  const sellVolume = column(table, 'sell_volume')
  const ofi = column(table, 'ofi')

  const rows: OfiBucket[] = new Array(table.numRows)
  for (let i = 0; i < table.numRows; i++) {
    rows[i] = {
      bucket: Number(bucket.get(i)),
      buyVolume: Number.parseFloat(buyVolume.get(i) as string),
      sellVolume: Number.parseFloat(sellVolume.get(i) as string),
      ofi: Number(ofi.get(i)),
    }
  }
  return rows
}
