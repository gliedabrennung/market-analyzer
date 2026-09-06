/** One OHLCV candle, normalized from either the Arrow or JSON wire format
 * (`GET /ohlcv/{symbol}`, `market-analyzer-tz.md` FR-5.1).
 *
 * Per frontend-tz.md §2.3: prices/volumes are `number` here because this
 * shape feeds chart rendering only (pixel grid is coarser than 8 decimal
 * places anyway). Any user-facing money arithmetic (P&L, spreads) must go
 * through `decimal.js-light` from the original string, never through this
 * type.
 */
export interface OhlcvRow {
  openTime: number
  closeTime: number
  open: number
  high: number
  low: number
  close: number
  volume: number
  quoteVolume: number
  tradesCount: number
  isClosed: boolean
}

/** A tradable instrument from `GET /symbols`. `pricePrecision`/`qtyPrecision`
 * are frontend-tz.md BE-6, not yet sent by the backend — optional until
 * that lands. */
export interface SymbolInfo {
  exchange: string
  symbol: string
  baseAsset: string
  quoteAsset: string
  status: string
  pricePrecision?: number
  qtyPrecision?: number
}

/** The wire shape of one `GET /symbols` row (snake_case, matches
 * `ma_api::routes::symbols::SymbolDto`'s `Serialize` output exactly). */
export interface SymbolInfoWire {
  exchange: string
  symbol: string
  base_asset: string
  quote_asset: string
  status: string
  price_precision?: number
  qty_precision?: number
}

export type Interval = '1m' | '5m' | '15m' | '1h' | '4h' | '1d'

export const INTERVALS: readonly Interval[] = ['1m', '5m', '15m', '1h', '4h', '1d']

/** The FR-5.2 error envelope every backend error response uses. */
export interface ApiErrorBody {
  error: {
    code: string
    message: string
    details: unknown
  }
}
