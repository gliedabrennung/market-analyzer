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

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080'

const MAX_PAGE_SIZE = 10_000

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

export function fetchCorrelation(
  params: CorrelationParams,
  options?: FetchRowsOptions,
): Promise<CorrelationPair[]> {
  const query = new URLSearchParams({ symbols: params.symbols.join(','), interval: params.interval })
  const path = `${API_BASE_URL}/analytics/correlation?${query}`
  return fetchRows(path, parseCorrelationArrow, options)
}

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
