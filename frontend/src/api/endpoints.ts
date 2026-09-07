import {
  parseAnomaliesArrow,
  parseCorrelationArrow,
  parseOfiArrow,
  parseOhlcvArrow,
  parseVolatilityArrow,
  parseVwapArrow,
} from './arrow'
import { fetchRows } from './client'
import type { FetchRowsOptions } from './client'
import type {
  CorrelationPair,
  Interval,
  OfiBucket,
  OhlcvRow,
  SymbolInfo,
  SymbolInfoWire,
  VolatilityPoint,
  VolumeAnomaly,
  VwapPoint,
} from './types'

/** `market-analyzer serve`'s default port (README.md); override with
 * `VITE_API_BASE_URL` for any other deployment. */
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080'

/** `ma_api::state::ApiLimits.max_pagination_limit` (`crates/api/src/state.rs`,
 * currently `10_000`, set in `crates/cli/src/commands/serve.rs`) — the most
 * rows any single request can return. A 30-day range at `1m` is ~43,200
 * candles, well past that, so every paginated endpoint below fetches
 * *all* pages rather than exposing `limit`/`offset` to callers: a single
 * request capped at this size would silently return only the oldest
 * page of the requested window (ascending order, offset 0), cutting off
 * the most recent data — worse than slow, actually wrong. */
const MAX_PAGE_SIZE = 10_000

/** Fetches every page of a paginated endpoint and concatenates them,
 * stopping at the first short page (fewer than `MAX_PAGE_SIZE` rows) —
 * the same "did we get everything" signal LIMIT/OFFSET pagination always
 * gives, without needing a separate total-count call. */
async function fetchAllPages<T>(fetchPage: (offset: number, limit: number) => Promise<T[]>): Promise<T[]> {
  const all: T[] = []
  let offset = 0
  for (;;) {
    const page = await fetchPage(offset, MAX_PAGE_SIZE)
    all.push(...page)
    if (page.length < MAX_PAGE_SIZE) break
    offset += MAX_PAGE_SIZE
  }
  return all
}

export interface OhlcvParams {
  symbol: string
  interval: Interval
  from: string
  to: string
}

/** `GET /ohlcv/{symbol}` (FR-5.1), all pages within `[from, to)`. */
export function fetchOhlcv(params: OhlcvParams, options?: FetchRowsOptions): Promise<OhlcvRow[]> {
  return fetchAllPages((offset, limit) => {
    const query = new URLSearchParams({
      interval: params.interval,
      from: params.from,
      to: params.to,
      limit: String(limit),
      offset: String(offset),
    })
    const path = `${API_BASE_URL}/ohlcv/${encodeURIComponent(params.symbol)}?${query}`
    return fetchRows(path, parseOhlcvArrow, options)
  })
}

export interface VwapParams {
  symbol: string
  interval: Interval
  window?: number
}

/** `GET /analytics/{symbol}/vwap` (FR-3.2, default window 20). The
 * backend's vwap query itself has no date-range filter — it scans full
 * symbol/interval history server-side regardless (see the root README's
 * "Известные ограничения") — so this fetches every page of *all* of it.
 * Correct, but genuinely slow for a symbol with years of history; that's
 * the backend's own disclosed limitation, not something pagination here
 * can fix. */
export function fetchVwap(params: VwapParams, options?: FetchRowsOptions): Promise<VwapPoint[]> {
  return fetchAllPages((offset, limit) => {
    const query = new URLSearchParams({
      interval: params.interval,
      limit: String(limit),
      offset: String(offset),
    })
    if (params.window !== undefined) query.set('window', String(params.window))
    const path = `${API_BASE_URL}/analytics/${encodeURIComponent(params.symbol)}/vwap?${query}`
    return fetchRows(path, parseVwapArrow, options)
  })
}

export interface VolatilityParams {
  symbol: string
  interval: Interval
  window?: number
}

/** `GET /analytics/{symbol}/volatility` (FR-3.3, default window 20). See
 * `fetchVwap`'s doc — same unbounded-history/full-pagination trade-off. */
export function fetchVolatility(
  params: VolatilityParams,
  options?: FetchRowsOptions,
): Promise<VolatilityPoint[]> {
  return fetchAllPages((offset, limit) => {
    const query = new URLSearchParams({
      interval: params.interval,
      limit: String(limit),
      offset: String(offset),
    })
    if (params.window !== undefined) query.set('window', String(params.window))
    const path = `${API_BASE_URL}/analytics/${encodeURIComponent(params.symbol)}/volatility?${query}`
    return fetchRows(path, parseVolatilityArrow, options)
  })
}

export interface AnomaliesParams {
  symbol: string
  interval: Interval
  window?: number
  threshold?: number
}

/** `GET /analytics/{symbol}/anomalies` (FR-3.4, defaults window 100 /
 * threshold 3.0). See `fetchVwap`'s doc — same unbounded-history/full-
 * pagination trade-off. */
export function fetchAnomalies(
  params: AnomaliesParams,
  options?: FetchRowsOptions,
): Promise<VolumeAnomaly[]> {
  return fetchAllPages((offset, limit) => {
    const query = new URLSearchParams({
      interval: params.interval,
      limit: String(limit),
      offset: String(offset),
    })
    if (params.window !== undefined) query.set('window', String(params.window))
    if (params.threshold !== undefined) query.set('threshold', String(params.threshold))
    const path = `${API_BASE_URL}/analytics/${encodeURIComponent(params.symbol)}/anomalies?${query}`
    return fetchRows(path, parseAnomaliesArrow, options)
  })
}

export interface OfiParams {
  symbol: string
  bucketSeconds?: number
  from: string
  to: string
}

/** `GET /analytics/{symbol}/ofi` (FR-3.5, default 60s bucket), all pages
 * within `[from, to)`. `from`/`to` are RFC3339 (OFI operates on the
 * `trades` dataset, not day-granularity klines, so it takes timestamps
 * rather than dates). */
export function fetchOfi(params: OfiParams, options?: FetchRowsOptions): Promise<OfiBucket[]> {
  return fetchAllPages((offset, limit) => {
    const query = new URLSearchParams({
      from: params.from,
      to: params.to,
      limit: String(limit),
      offset: String(offset),
    })
    if (params.bucketSeconds !== undefined) query.set('bucket', String(params.bucketSeconds))
    const path = `${API_BASE_URL}/analytics/${encodeURIComponent(params.symbol)}/ofi?${query}`
    return fetchRows(path, parseOfiArrow, options)
  })
}

export interface CorrelationParams {
  symbols: string[]
  interval: Interval
}

/** `GET /analytics/correlation` (FR-3.6/FR-6.1). No `limit`/`offset` on
 * the backend — it's capped server-side at
 * `ApiLimits.max_correlation_symbols` (10) symbols, so at most C(10,2) =
 * 45 rows ever come back; pagination would be pure overhead. */
export function fetchCorrelation(
  params: CorrelationParams,
  options?: FetchRowsOptions,
): Promise<CorrelationPair[]> {
  const query = new URLSearchParams({ symbols: params.symbols.join(','), interval: params.interval })
  const path = `${API_BASE_URL}/analytics/correlation?${query}`
  return fetchRows(path, parseCorrelationArrow, options)
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
    hasData: s.has_data ?? false,
    pricePrecision: s.price_precision,
    qtyPrecision: s.qty_precision,
  }))
}
