import {
  createEffect,
  createMemo,
  createSignal,
  ErrorBoundary,
  lazy,
  on,
  onCleanup,
  Show,
  Suspense,
} from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import Download from 'lucide-solid/icons/download'
import { PriceChart } from './charts/price-chart'
import { fetchOhlcv, fetchVwap } from './api/endpoints'
import type { OhlcvRow } from './api/types'
import { INTERVALS } from './api/types'
import { SymbolPicker } from './panels/symbol-picker'
import { VolatilityPanel } from './panels/volatility-panel'
import { VolumeZScorePanel } from './panels/volume-zscore-panel'
// NFR-2.6/README's own tight-budget note: off by default (state/settings.ts)
// and never part of the first render — a real dynamic import(), not just
// deferred rendering, keeps its code out of the initial JS chunk.
const CorrelationMatrix = lazy(() => import('./panels/correlation-matrix'))
// Same reasoning: never mounted on first paint, the default sidebar tab
// is "Сделки" (see SIDEBAR_TABS below).
const AnomaliesTable = lazy(() => import('./panels/anomalies-table'))
// Same reasoning: off by default (state/settings.ts).
const OfiPanel = lazy(() => import('./panels/ofi-panel'))
// See trade-tape.tsx's own comment on its default export — lazy despite
// being the default-shown sidebar tab.
const TradeTape = lazy(() => import('./panels/trade-tape'))
import { StatusBar } from './panels/status-bar'
import { IntervalSwitcher } from './ui/interval-switcher'
import { ToggleCheckbox } from './ui/toggle-checkbox'
import { ThemeControls } from './ui/theme-toggle'
import { ChartSkeleton, PanelSkeleton } from './ui/skeleton'
import { Tabs } from './ui/tabs'
import { interval, setInterval, setSymbol, symbol } from './state/selection'
import { indicatorSettings, toggleIndicator } from './state/settings'
import { useLiveStream } from './stream/useLiveStream'

const HISTORY_DAYS = 30
const HISTORY_EXTEND_DAYS = 30
const MS_PER_DAY = 24 * 60 * 60 * 1000
const VWAP_WINDOW = 20
const SIDEBAR_TABS = ['Сделки', 'Аномалии'] as const

function isoDate(date: Date): string {
  return date.toISOString().slice(0, 10)
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
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
  const priceByTime = createMemo(() => new Map(chartData().map((r) => [r.openTime, r.close])))

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

  // FR-5.3: clicking an anomalies-table row jumps the chart there.
  const [jumpTarget, setJumpTarget] = createSignal<{ time: number; nonce: number } | null>(null)
  function jumpToTime(time: number) {
    setJumpTarget({ time, nonce: Date.now() })
  }

  // `r` hotkey (FR-8.5).
  const [resetZoomNonce, setResetZoomNonce] = createSignal<number | undefined>(undefined)

  const [sidebarTab, setSidebarTab] = createSignal<(typeof SIDEBAR_TABS)[number]>('Сделки')

  // FR-8.5: `/` symbol search, `1`-`6` interval, `Esc` close/blur, `r`
  // reset zoom. Suppressed while the user is actually typing somewhere
  // (an input/textarea/contenteditable) — except Escape, which should
  // always be able to get you *out* of one.
  function onGlobalKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
      return
    }
    if (isTypingTarget(e.target)) return
    if (e.key === '/') {
      e.preventDefault()
      document.querySelector<HTMLInputElement>('[data-hotkey-target="symbol-search"]')?.focus()
    } else if (e.key >= '1' && e.key <= '6') {
      const target = INTERVALS[Number(e.key) - 1]
      if (target !== undefined) setInterval(target)
    } else if (e.key === 'r') {
      setResetZoomNonce((n) => (n ?? 0) + 1)
    }
  }
  document.addEventListener('keydown', onGlobalKeyDown)
  onCleanup(() => document.removeEventListener('keydown', onGlobalKeyDown))

  // NFR-2.6: dynamically imported — `export/csv.ts`'s own comment has the
  // full reasoning. Only ever reached by an explicit click.
  async function exportCsv() {
    const { buildOhlcvCsv, downloadCsv } = await import('./export/csv')
    downloadCsv(`${symbol()}_${interval()}.csv`, buildOhlcvCsv(chartData()))
  }

  return (
    <div class="flex h-screen min-w-[1280px] flex-col bg-[var(--color-bg)]">
      <header class="flex flex-wrap items-center gap-4 border-b border-[var(--color-border)] px-4 py-2">
        <h1 class="text-sm font-semibold text-[var(--color-fg)]">Market Analyzer</h1>
        <SymbolPicker value={symbol()} onSelect={setSymbol} />
        <IntervalSwitcher value={interval()} onChange={setInterval} />
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
          <ToggleCheckbox
            label="Correlation"
            checked={indicatorSettings().correlation}
            onChange={() => toggleIndicator('correlation')}
          />
        </div>
        <ThemeControls />
        <button
          type="button"
          onClick={() => void exportCsv()}
          disabled={chartData().length === 0}
          title="Экспорт загруженных свечей в CSV"
          class="ml-auto flex items-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-surface-2)] hover:text-[var(--color-fg)] disabled:pointer-events-none disabled:opacity-40"
        >
          <Download size={14} />
          CSV
        </button>
      </header>

      <StatusBar
        symbol={symbol()}
        rows={chartData()}
        liveKline={liveStream.liveKline()}
        connectionState={liveStream.connectionState()}
      />

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
                    jumpTarget={jumpTarget()}
                    resetZoomNonce={resetZoomNonce()}
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
                <Suspense fallback={<PanelSkeleton />}>
                  <OfiPanel symbol={symbol()} />
                </Suspense>
              </Show>
            </ErrorBoundary>
            <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
              <Show when={indicatorSettings().anomalies}>
                <VolumeZScorePanel symbol={symbol()} interval={interval()} />
              </Show>
            </ErrorBoundary>
            <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
              <Show when={indicatorSettings().correlation}>
                <Suspense fallback={<PanelSkeleton />}>
                  <CorrelationMatrix interval={interval()} />
                </Suspense>
              </Show>
            </ErrorBoundary>
          </div>
        </div>

        {/* DR-7: side column — trade tape and anomalies table, as tabs. */}
        <aside class="w-80 shrink-0 border-l border-[var(--color-border)]">
          <Tabs
            tabs={SIDEBAR_TABS}
            active={sidebarTab()}
            onChange={(t) => setSidebarTab(t as (typeof SIDEBAR_TABS)[number])}
          >
            <Show when={sidebarTab() === 'Сделки'}>
              <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
                <div class="h-full p-2">
                  <Suspense fallback={<PanelSkeleton />}>
                    <TradeTape trades={liveStream.trades()} />
                  </Suspense>
                </div>
              </ErrorBoundary>
            </Show>
            <Show when={sidebarTab() === 'Аномалии'}>
              <ErrorBoundary fallback={(error) => <PanelError error={error} />}>
                <Suspense fallback={<PanelSkeleton />}>
                  <AnomaliesTable
                    symbol={symbol()}
                    interval={interval()}
                    priceByTime={priceByTime()}
                    onSelect={jumpToTime}
                  />
                </Suspense>
              </ErrorBoundary>
            </Show>
          </Tabs>
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
