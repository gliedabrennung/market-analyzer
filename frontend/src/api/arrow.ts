import { tableFromIPC, type Table } from 'apache-arrow'
import type { OhlcvRow } from './types'

/** Reads a named column off `table`, or throws — a missing column means the
 * backend's schema and this parser have drifted, which should fail loudly
 * in dev rather than silently produce `NaN`/`undefined` rows. */
function column(table: Table, name: string) {
  const col = table.getChild(name)
  if (col === null) {
    throw new Error(`Arrow response is missing expected column '${name}'`)
  }
  return col
}

/** Parses an `/ohlcv/*` Arrow IPC stream response (frontend-tz.md FR-1.1).
 *
 * Reads each column once as its native typed value (millisecond epoch
 * numbers for timestamps, strings for the `Utf8`-encoded decimals — see
 * `types.ts`) rather than routing through `JSON.parse` — the whole point
 * of the Arrow transport (frontend-tz.md §2.2).
 */
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
