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

/** `GET /analytics/{symbol}/vwap` (FR-3.2). `vwap` is `null` for the
 * leading bars of a window with no volume yet. */
export interface VwapPoint {
  openTime: number
  close: number
  vwap: number | null
}

/** `GET /analytics/{symbol}/volatility` (FR-3.3). `null` where fewer than
 * 2 log returns exist in the window yet. */
export interface VolatilityPoint {
  openTime: number
  realizedVolatility: number | null
}

/** `GET /analytics/{symbol}/anomalies` (FR-3.4): one detected volume spike. */
export interface VolumeAnomaly {
  openTime: number
  volume: number
  zScore: number
}

/** `GET /analytics/{symbol}/ofi` (FR-3.5): one Order Flow Imbalance bucket. */
export interface OfiBucket {
  bucket: number
  buyVolume: number
  sellVolume: number
  ofi: number
}

/** `GET /analytics/correlation` (FR-3.6/FR-6.1): one symbol pair's
 * correlation coefficient. `null` when the pair never has two
 * overlapping non-null returns. */
export interface CorrelationPair {
  symbolA: string
  symbolB: string
  correlation: number | null
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
