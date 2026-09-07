import type { Interval, OhlcvRow } from '../api/types'

export type ConnectionState = 'connecting' | 'live' | 'reconnecting' | 'offline'

/** One trade tick, FR-7.1's fields (время/цена/объём/сторона агрессора). */
export interface LiveTrade {
  ts: number
  tradeId: number
  price: number
  qty: number
  /** `true`: the aggressor (taker) was the seller. Matches `ma_core::Trade`. */
  isBuyerMaker: boolean
}

export interface LiveSocketHandlers {
  onTrade: (trade: LiveTrade) => void
  onKline: (row: OhlcvRow) => void
  onStateChange: (state: ConnectionState) => void
}

const BACKOFF_BASE_MS = 500
const BACKOFF_MULTIPLIER = 2
const BACKOFF_MAX_MS = 15_000

function wsUrl(symbol: string, interval: Interval): string {
  const httpBase: string = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080'
  const wsBase = httpBase.replace(/^http/, 'ws')
  return `${wsBase}/stream/${encodeURIComponent(symbol)}?interval=${interval}`
}

export type ParsedMessage =
  { kind: 'trade'; trade: LiveTrade } | { kind: 'kline'; row: OhlcvRow } | { kind: 'heartbeat' }

/** Parses one `/stream/{symbol}` WS text frame (frontend-tz.md BE-5:
 * `{"type": "trade" | "kline" | "heartbeat", ...}`, verified against the
 * real backend's bytes during development — see `ma_core::MarketEvent`'s
 * `#[serde(tag = "type")]`). Decimal fields (price/qty/OHLCV) arrive as
 * strings, same reasoning as the REST Arrow columns (frontend-tz.md
 * §2.3) — parsed to `number` here since this only ever feeds chart
 * rendering. Returns `null` for anything unrecognized rather than
 * throwing — a forward-compatible new message `type` shouldn't kill the
 * connection. */
export function parseLiveMessage(raw: string): ParsedMessage | null {
  let obj: unknown
  try {
    obj = JSON.parse(raw)
  } catch {
    return null
  }
  if (typeof obj !== 'object' || obj === null) return null
  const o = obj as Record<string, unknown>

  if (o.type === 'trade') {
    return {
      kind: 'trade',
      trade: {
        ts: Date.parse(o.ts as string),
        tradeId: Number(o.trade_id),
        price: Number.parseFloat(o.price as string),
        qty: Number.parseFloat(o.qty as string),
        isBuyerMaker: Boolean(o.is_buyer_maker),
      },
    }
  }
  if (o.type === 'kline') {
    return {
      kind: 'kline',
      row: {
        openTime: Date.parse(o.open_time as string),
        closeTime: Date.parse(o.close_time as string),
        open: Number.parseFloat(o.open as string),
        high: Number.parseFloat(o.high as string),
        low: Number.parseFloat(o.low as string),
        close: Number.parseFloat(o.close as string),
        volume: Number.parseFloat(o.volume as string),
        quoteVolume: Number.parseFloat(o.quote_volume as string),
        tradesCount: Number(o.trades_count),
        isClosed: Boolean(o.is_closed),
      },
    }
  }
  if (o.type === 'heartbeat') {
    return { kind: 'heartbeat' }
  }
  return null
}

/** FR-2.1/2.2: connects to `/stream/{symbol}`, reconnecting with
 * exponential backoff (base 500ms, ×2, capped at 15s, unlimited retries)
 * on any close/error that wasn't a deliberate `close()` call. The backoff
 * resets to base once a connection actually opens.
 *
 * Framework-agnostic (plain callbacks, no Solid) — `../stream/useLiveStream`
 * wires this into reactive state and rAF batching.
 */
export function connectLiveSocket(
  symbol: string,
  interval: Interval,
  handlers: LiveSocketHandlers,
): { close: () => void } {
  let socket: WebSocket | null = null
  let reconnectTimer: number | null = null
  let backoffMs = BACKOFF_BASE_MS
  let deliberatelyClosed = false

  function open() {
    socket = new WebSocket(wsUrl(symbol, interval))

    socket.onopen = () => {
      backoffMs = BACKOFF_BASE_MS
      handlers.onStateChange('live')
    }

    socket.onmessage = (event) => {
      const parsed = parseLiveMessage(event.data as string)
      if (parsed === null || parsed.kind === 'heartbeat') return
      if (parsed.kind === 'trade') handlers.onTrade(parsed.trade)
      else handlers.onKline(parsed.row)
    }

    socket.onclose = () => {
      if (deliberatelyClosed) return
      scheduleReconnect()
    }

    // A WebSocket error is always followed by its close event — let
    // onclose be the single place that decides to reconnect.
    socket.onerror = () => {}
  }

  function scheduleReconnect() {
    handlers.onStateChange('reconnecting')
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = null
      backoffMs = Math.min(backoffMs * BACKOFF_MULTIPLIER, BACKOFF_MAX_MS)
      open()
    }, backoffMs)
  }

  handlers.onStateChange('connecting')
  open()

  return {
    close() {
      deliberatelyClosed = true
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer)
        reconnectTimer = null
      }
      socket?.close()
      handlers.onStateChange('offline')
    },
  }
}
