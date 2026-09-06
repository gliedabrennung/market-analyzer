import { createMemo, ErrorBoundary, Show } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { PriceChart } from './charts/price-chart'
import { fetchOhlcv } from './api/endpoints'
import { SymbolPicker } from './panels/symbol-picker'
import { IntervalSwitcher } from './ui/interval-switcher'
import { ChartSkeleton } from './ui/skeleton'
import { interval, setInterval, setSymbol, symbol } from './state/selection'

const HISTORY_DAYS = 30
const MS_PER_DAY = 24 * 60 * 60 * 1000

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
      fetchOhlcv(
        { symbol: symbol(), interval: interval(), from: range().from, to: range().to, limit: 10_000 },
        { signal },
      ),
  }))

  return (
    <div class="flex h-screen min-w-[1280px] flex-col bg-[var(--color-bg)]">
      <header class="flex items-center gap-4 border-b border-[var(--color-border)] px-4 py-2">
        <h1 class="text-sm font-semibold text-[var(--color-fg)]">Market Analyzer</h1>
        <SymbolPicker value={symbol()} onSelect={setSymbol} />
        <IntervalSwitcher value={interval()} onChange={setInterval} />
      </header>

      <main class="min-h-0 flex-1 p-2">
        <ErrorBoundary
          fallback={(error) => (
            <div class="flex h-full items-center justify-center text-sm text-[var(--color-down)]">
              Не удалось загрузить график: {String(error)}
            </div>
          )}
        >
          <Show when={!ohlcvQuery.isLoading} fallback={<ChartSkeleton />}>
            <Show
              when={(ohlcvQuery.data?.length ?? 0) > 0}
              fallback={
                <div class="flex h-full items-center justify-center text-sm text-[var(--color-fg-muted)]">
                  Нет данных по {symbol()} за последние {HISTORY_DAYS} дней
                </div>
              }
            >
              <PriceChart data={ohlcvQuery.data ?? []} />
            </Show>
          </Show>
        </ErrorBoundary>
      </main>
    </div>
  )
}
