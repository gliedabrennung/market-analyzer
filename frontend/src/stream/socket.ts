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
  /** `symbol`/`interval` are the frame's own, not the subscription's — the
   * consumer checks them against what it is currently displaying. */
  onTrade: (symbol: string, trade: LiveTrade) => void
  onKline: (symbol: string, interval: string, row: OhlcvRow) => void
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
  | { kind: 'trade'; symbol: string; trade: LiveTrade }
  | { kind: 'kline'; symbol: string; interval: string; row: OhlcvRow }
  | { kind: 'heartbeat' }

/** Every numeric field a chart consumes has to be a real number. A frame
 * missing one (or carrying something unparseable) yields `NaN` here, and
 * Lightweight Charts rejects `NaN` by *throwing* from inside the effect
 * that applies it — which takes the whole chart panel down until a page
 * reload. Dropping the frame instead costs one tick. */
function finite(value: unknown): number | null {
  const n = typeof value === 'string' ? Number.parseFloat(value) : Number(value)
  return Number.isFinite(n) ? n : null
}

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
    const ts = finite(Date.parse(o.ts as string))
    const price = finite(o.price)
    const qty = finite(o.qty)
    if (ts === null || price === null || qty === null || typeof o.symbol !== 'string') return null
    return {
      kind: 'trade',
      symbol: o.symbol,
      trade: {
        ts,
        tradeId: Number(o.trade_id),
        price,
        qty,
        isBuyerMaker: Boolean(o.is_buyer_maker),
      },
    }
  }
  if (o.type === 'kline') {
    const numbers = {
      openTime: finite(Date.parse(o.open_time as string)),
      closeTime: finite(Date.parse(o.close_time as string)),
      open: finite(o.open),
      high: finite(o.high),
      low: finite(o.low),
      close: finite(o.close),
      volume: finite(o.volume),
      quoteVolume: finite(o.quote_volume),
    }
    if (
      typeof o.symbol !== 'string' ||
      typeof o.interval !== 'string' ||
      Object.values(numbers).some((v) => v === null)
    ) {
      return null
    }
    return {
      kind: 'kline',
      symbol: o.symbol,
      interval: o.interval,
      row: {
        openTime: numbers.openTime as number,
        closeTime: numbers.closeTime as number,
        open: numbers.open as number,
        high: numbers.high as number,
        low: numbers.low as number,
        close: numbers.close as number,
        volume: numbers.volume as number,
        quoteVolume: numbers.quoteVolume as number,
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
      // A frame can already be queued when `close()` is called (switching
      // symbols does exactly that), and delivering it afterwards feeds the
      // *previous* pair's data to the handlers now wired to the new one.
      if (deliberatelyClosed) return
      const parsed = parseLiveMessage(event.data as string)
      if (parsed === null || parsed.kind === 'heartbeat') return
      if (parsed.kind === 'trade') handlers.onTrade(parsed.symbol, parsed.trade)
      else handlers.onKline(parsed.symbol, parsed.interval, parsed.row)
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
