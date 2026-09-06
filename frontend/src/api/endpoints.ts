import { parseOhlcvArrow } from './arrow'
import { fetchRows } from './client'
import type { FetchRowsOptions } from './client'
import type { Interval, OhlcvRow, SymbolInfo, SymbolInfoWire } from './types'

/** `market-analyzer serve`'s default port (README.md); override with
 * `VITE_API_BASE_URL` for any other deployment. */
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080'

export interface OhlcvParams {
  symbol: string
  interval: Interval
  from: string
  to: string
  limit?: number
  offset?: number
}

/** `GET /ohlcv/{symbol}` (FR-5.1). */
export function fetchOhlcv(params: OhlcvParams, options?: FetchRowsOptions): Promise<OhlcvRow[]> {
  const query = new URLSearchParams({ interval: params.interval, from: params.from, to: params.to })
  if (params.limit !== undefined) query.set('limit', String(params.limit))
  if (params.offset !== undefined) query.set('offset', String(params.offset))

  const path = `${API_BASE_URL}/ohlcv/${encodeURIComponent(params.symbol)}?${query}`
  return fetchRows(path, parseOhlcvArrow, options)
}

/** `GET /symbols` (FR-5.1) — JSON only, it's a small registry, not a
 * high-volume time series, so Arrow buys nothing here. */
export async function fetchSymbols(options?: FetchRowsOptions): Promise<SymbolInfo[]> {
  const res = await fetch(`${API_BASE_URL}/symbols`, {
    headers: { Accept: 'application/json' },
    signal: options?.signal,
  })
  if (!res.ok) {
    throw new Error(`GET /symbols failed: HTTP ${res.status}`)
  }
  const wire = (await res.json()) as SymbolInfoWire[]
  return wire.map((s) => ({
    exchange: s.exchange,
    symbol: s.symbol,
    baseAsset: s.base_asset,
    quoteAsset: s.quote_asset,
    status: s.status,
    pricePrecision: s.price_precision,
    qtyPrecision: s.qty_precision,
  }))
}
