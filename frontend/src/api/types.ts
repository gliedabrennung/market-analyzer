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

export interface SymbolInfo {
  exchange: string
  symbol: string
  baseAsset: string
  quoteAsset: string
  status: string

  hasData: boolean
  pricePrecision?: number
  qtyPrecision?: number
}

export interface SymbolInfoWire {
  exchange: string
  symbol: string
  base_asset: string
  quote_asset: string
  status: string
  has_data: boolean
  price_precision?: number
  qty_precision?: number
}

export interface VwapPoint {
  openTime: number
  close: number
  vwap: number | null
}

export interface VolatilityPoint {
  openTime: number
  realizedVolatility: number | null
}

export interface VolumeAnomaly {
  openTime: number
  volume: number
  zScore: number
}

export interface OfiBucket {
  bucket: number
  buyVolume: number
  sellVolume: number
  ofi: number
}

export interface CorrelationPair {
  symbolA: string
  symbolB: string
  correlation: number | null
}

export type Interval = '1m' | '5m' | '15m' | '1h' | '4h' | '1d'

export const INTERVALS: readonly Interval[] = ['1m', '5m', '15m', '1h', '4h', '1d']

export interface ApiErrorBody {
  error: {
    code: string
    message: string
    details: unknown
  }
}
