import { createEffect, createSignal, onCleanup } from 'solid-js'
import { connectLiveSocket } from './socket'
import type { ConnectionState, LiveTrade } from './socket'
import { createRafBuffer } from './buffer'
import type { Interval, OhlcvRow } from '../api/types'

/** FR-2.4/FR-7.2: the trade tape only ever shows the last 500. */
const TRADE_TAPE_CAPACITY = 500

export interface UseLiveStreamOptions {
  symbol: () => string
  interval: () => Interval
  /** FR-2.5: called once the tab becomes visible again after being
   * hidden. The connection itself was torn down while hidden (not kept
   * open and merely ignored) — so "catch up" here means one REST
   * refetch, not replaying anything buffered client-side, because
   * nothing was buffered. */
  onResume?: () => void
}

export interface LiveStream {
  connectionState: () => ConnectionState
  trades: () => LiveTrade[]
  liveKline: () => OhlcvRow | null
}

/** FR-2.1..2.5: owns one live WS connection for the current
 * symbol/interval, batches its messages through rAF, and exposes the
 * result as plain Solid signals. */
export function useLiveStream(options: UseLiveStreamOptions): LiveStream {
  const [connectionState, setConnectionState] = createSignal<ConnectionState>('offline')
  const [trades, setTrades] = createSignal<LiveTrade[]>([])
  const [liveKline, setLiveKline] = createSignal<OhlcvRow | null>(null)

  const tradeBuffer = createRafBuffer<LiveTrade>({
    keepOnOverflow: TRADE_TAPE_CAPACITY,
    onFlush: (items) => {
      setTrades((prev) => [...prev, ...items].slice(-TRADE_TAPE_CAPACITY))
    },
  })
  const klineBuffer = createRafBuffer<OhlcvRow>({
    keepOnOverflow: 1,
    onFlush: (items) => {
      const last = items.at(-1)
      if (last !== undefined) setLiveKline(last)
    },
  })

  let handle: { close: () => void } | null = null

  function start() {
    handle?.close()
    // Both buffers hold whatever the *previous* pair sent but the browser
    // has not painted yet. Flushing that into the new pair's chart applies
    // one pair's price to another's series — visibly a single candle far
    // above or below everything else, with the price scale stretched to
    // match. `stop()` drops the queued items along with the pending frame.
    tradeBuffer.stop()
    klineBuffer.stop()
    setLiveKline(null)
    handle = connectLiveSocket(options.symbol(), options.interval(), {
      // Second line of defence, for a frame that arrives after the switch
      // on a socket that has not finished closing: only take what matches
      // what is on screen right now.
      onTrade: (frameSymbol, t) => {
        if (frameSymbol !== options.symbol()) return
        tradeBuffer.push(t)
      },
      onKline: (frameSymbol, frameInterval, row) => {
        if (frameSymbol !== options.symbol() || frameInterval !== options.interval()) return
        klineBuffer.push(row)
      },
      onStateChange: setConnectionState,
    })
  }

  function stop() {
    handle?.close()
    handle = null
    tradeBuffer.stop()
    klineBuffer.stop()
    setConnectionState('offline')
  }

  // FR-2.1: fresh connection on symbol/interval change (old one closed
  // first, inside `start()`).
  createEffect(() => {
    options.symbol()
    options.interval()
    setTrades([])
    if (!document.hidden) start()
  })

  function onVisibilityChange() {
    if (document.hidden) {
      stop()
    } else {
      start()
      options.onResume?.()
    }
  }
  document.addEventListener('visibilitychange', onVisibilityChange)

  onCleanup(() => {
    document.removeEventListener('visibilitychange', onVisibilityChange)
    stop()
  })

  return { connectionState, trades, liveKline }
}
