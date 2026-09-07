import { createEffect, createMemo, createSignal, ErrorBoundary, on, Show } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { PriceChart } from './charts/price-chart'
import { fetchOhlcv, fetchVwap } from './api/endpoints'
import type { OhlcvRow } from './api/types'
import { SymbolPicker } from './panels/symbol-picker'
import { VolatilityPanel } from './panels/volatility-panel'
import { OfiPanel } from './panels/ofi-panel'
import { VolumeZScorePanel } from './panels/volume-zscore-panel'
import { TradeTape } from './panels/trade-tape'
import { IntervalSwitcher } from './ui/interval-switcher'
import { ToggleCheckbox } from './ui/toggle-checkbox'
import { ChartSkeleton } from './ui/skeleton'
import { ConnectionStatus } from './ui/connection-status'
import { interval, setInterval, setSymbol, symbol } from './state/selection'
import { indicatorSettings, toggleIndicator } from './state/settings'
import { useLiveStream } from './stream/useLiveStream'

const HISTORY_DAYS = 30
const HISTORY_EXTEND_DAYS = 30
const MS_PER_DAY = 24 * 60 * 60 * 1000
const VWAP_WINDOW = 20

function isoDate(date: Date): string {
  return date.toISOString().slice(0, 10)
}

export function App() {
  const range = createMemo(() => {
    const to = new Date()
    const from = new Date(to.getTime() - HISTORY_DAYS * MS_PER_DAY)
    return { from: isoDate(from), to: isoDate(to) }
  })

  const ohlcvQuery = createQuery(() => ({
    queryKey: ['ohlcv', symbol(), interval(), range().from, range().to],
    queryFn: ({ signal }) =>
      fetchOhlcv({ symbol: symbol(), interval: interval(), from: range().from, to: range().to }, { signal }),
  }))

  // FR-3.5: history loaded beyond the base `range()` window, prepended as
  // the user scrolls left. Kept outside solid-query's cache (which is
  // keyed by a fixed `[from, to]` and would otherwise treat every
  // extension as a brand new, ever-growing fetch) — a plain accumulator
  // that only ever grows by fetching the *next* older slice.
  const [extraOlderRows, setExtraOlderRows] = createSignal<OhlcvRow[]>([])
  // Seeding the initial value only; every subsequent change is handled
  // explicitly by the `on([symbol, interval], ...)` effect below, which
  // re-reads `range()`.
  // eslint-disable-next-line solid/reactivity
  const [earliestLoaded, setEarliestLoaded] = createSignal(range().from)
  const [loadingEarlier, setLoadingEarlier] = createSignal(false)
  const [historyExhausted, setHistoryExhausted] = createSignal(false)

  createEffect(
    on([symbol, interval], () => {
      setExtraOlderRows([])
      setEarliestLoaded(range().from)
      setHistoryExhausted(false)
    }),
  )

  async function loadEarlierHistory() {
    if (loadingEarlier() || historyExhausted()) return
    setLoadingEarlier(true)
    try {
      const to = earliestLoaded()
      const from = isoDate(new Date(new Date(to).getTime() - HISTORY_EXTEND_DAYS * MS_PER_DAY))
      const rows = await fetchOhlcv({ symbol: symbol(), interval: interval(), from, to })
      if (rows.length === 0) {
        setHistoryExhausted(true)
      } else {
        setExtraOlderRows((prev) => [...rows, ...prev])
        setEarliestLoaded(from)
      }
    } finally {
      setLoadingEarlier(false)
    }
  }

  const chartData = createMemo(() => [...extraOlderRows(), ...(ohlcvQuery.data ?? [])])

  const vwapQuery = createQuery(() => ({
    queryKey: ['vwap', symbol(), interval()],
    queryFn: ({ signal }) =>
      fetchVwap({ symbol: symbol(), interval: interval(), window: VWAP_WINDOW }, { signal }),
  }))

  // Этап 3: FR-2.1..2.5 live WS (reconnect, batching, tab-hidden pause),
  // FR-3.3 live candle updates, FR-7.x trade tape.
  const liveStream = useLiveStream({
    symbol,
    interval,
    onResume: () => void ohlcvQuery.refetch(),
  })

  return (
    <div class="flex h-screen min-w-[1280px] flex-col bg-[var(--color-bg)]">
      <header class="flex flex-wrap items-center gap-4 border-b border-[var(--color-border)] px-4 py-2">
        <h1 class="text-sm font-semibold text-[var(--color-fg)]">Market Analyzer</h1>
        <SymbolPicker value={symbol()} onSelect={setSymbol} />
        <IntervalSwitcher value={interval()} onChange={setInterval} />
        <ConnectionStatus state={liveStream.connectionState()} />
        <div class="flex items-center gap-3 border-l border-[var(--color-border)] pl-4">
          <ToggleCheckbox
            label="VWAP"
            checked={indicatorSettings().vwap}
            onChange={() => toggleIndicator('vwap')}
          />
          <ToggleCheckbox
            label="MA(20)"
            checked={indicatorSettings().ma}
            onChange={() => toggleIndicator('ma')}
          />
          <ToggleCheckbox
            label="Volatility"
            checked={indicatorSettings().volatility}
            onChange={() => toggleIndicator('volatility')}
          />
          <ToggleCheckbox
            label="OFI"
            checked={indicatorSettings().ofi}
            onChange={() => toggleIndicator('ofi')}
          />
          <ToggleCheckbox
            label="Z-Score"
            checked={indicatorSettings().anomalies}
            onChange={() => toggleIndicator('anomalies')}
          />
        </div>
      </header>

      <main class="flex min-h-0 flex-1 flex-row">
        <div class="flex min-h-0 min-w-0 flex-1 flex-col gap-2 p-2">
          <ErrorBoundary
            fallback={(error) => (
              <div class="flex h-[55vh] items-center justify-center text-sm text-[var(--color-down)]">
                Не удалось загрузить график: {String(error)}
              </div>
            )}
          >
            <div class="h-[55vh] min-h-0">
              <Show when={!ohlcvQuery.isLoading} fallback={<ChartSkeleton />}>
                <Show
                  when={(ohlcvQuery.data?.length ?? 0) > 0}
                  fallback={
                    <div class="flex h-full items-center justify-center text-sm text-[var(--color-fg-muted)]">
                      Нет данных по {symbol()} за последние {HISTORY_DAYS} дней
                    </div>
                  }
                >
                  <PriceChart
                    data={chartData()}
                    vwapData={vwapQuery.data}
                    showVwap={indicatorSettings().vwap}
                    showMa={indicatorSettings().ma}
                    liveKline={liveStream.liveKline()}
                    onLoadEarlier={loadEarlierHistory}
                  />
                </Show>
              </Show>
            </div>
          </ErrorBoundary>

          <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
            <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
              <Show when={indicatorSettings().volatility}>
                <VolatilityPanel symbol={symbol()} interval={interval()} />
              </Show>
            </ErrorBoundary>
            <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
              <Show when={indicatorSettings().ofi}>
                <OfiPanel symbol={symbol()} />
              </Show>
            </ErrorBoundary>
            <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
              <Show when={indicatorSettings().anomalies}>
                <VolumeZScorePanel symbol={symbol()} interval={interval()} />
              </Show>
            </ErrorBoundary>
          </div>
        </div>

        {/* DR-7: side column — trade tape now, anomalies table joins it in Этап 4. */}
        <aside class="w-72 shrink-0 border-l border-[var(--color-border)] p-2">
          <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
            <TradeTape trades={liveStream.trades()} />
          </ErrorBoundary>
        </aside>
      </main>
    </div>
  )
}

/** FR-1.5: one panel's error doesn't take the rest of the UI down with it. */
function PanelError(props: { error: unknown }) {
  return (
    <div class="rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] p-4 text-sm text-[var(--color-down)]">
      Панель не загрузилась: {String(props.error)}
    </div>
  )
}
