import { createEffect, createRoot, createSignal, on } from 'solid-js'
import { INTERVALS, type Interval } from '../api/types'

const DEFAULT_SYMBOL = 'BTCUSDT'
const DEFAULT_INTERVAL: Interval = '1m'

function isInterval(value: string | null): value is Interval {
  return value !== null && (INTERVALS as readonly string[]).includes(value)
}

function readFromUrl(): { symbol: string; interval: Interval } {
  const params = new URLSearchParams(window.location.search)
  const symbolParam = params.get('symbol')
  const intervalParam = params.get('interval')
  return {
    symbol: symbolParam && symbolParam.length > 0 ? symbolParam : DEFAULT_SYMBOL,
    interval: isInterval(intervalParam) ? intervalParam : DEFAULT_INTERVAL,
  }
}

const initial = readFromUrl()

export const [symbol, setSymbol] = createSignal(initial.symbol)
export const [interval, setInterval] = createSignal<Interval>(initial.interval)

createRoot(() => {
  createEffect(
    on(
      [symbol, interval],
      ([sym, ivl]) => {
        const params = new URLSearchParams(window.location.search)
        params.set('symbol', sym)
        params.set('interval', ivl)
        window.history.pushState(null, '', `${window.location.pathname}?${params}`)
      },
      { defer: true },
    ),
  )

  window.addEventListener('popstate', () => {
    const next = readFromUrl()
    setSymbol(next.symbol)
    setInterval(next.interval)
  })
})
