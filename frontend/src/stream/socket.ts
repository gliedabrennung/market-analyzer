import type { Interval, OhlcvRow } from '../api/types'

export type ConnectionState = 'connecting' | 'live' | 'reconnecting' | 'offline'

export interface LiveTrade {
  ts: number
  tradeId: number
  price: number
  qty: number

  isBuyerMaker: boolean
}

export interface LiveSocketHandlers {
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

function finite(value: unknown): number | null {
  const n = typeof value === 'string' ? Number.parseFloat(value) : Number(value)
  return Number.isFinite(n) ? n : null
}

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
