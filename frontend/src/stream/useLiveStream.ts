import { createEffect, createSignal, onCleanup } from 'solid-js'
import { connectLiveSocket } from './socket'
import type { ConnectionState, LiveTrade } from './socket'
import { createRafBuffer } from './buffer'
import type { Interval, OhlcvRow } from '../api/types'

const TRADE_TAPE_CAPACITY = 500

export interface UseLiveStreamOptions {
  symbol: () => string
  interval: () => Interval

  onResume?: () => void
}

export interface LiveStream {
  connectionState: () => ConnectionState
  trades: () => LiveTrade[]
  liveKline: () => OhlcvRow | null
}

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

    tradeBuffer.stop()
    klineBuffer.stop()
    setLiveKline(null)
    handle = connectLiveSocket(options.symbol(), options.interval(), {
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
