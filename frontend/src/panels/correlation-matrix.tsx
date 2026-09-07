import { createMemo, createSignal, For, Show } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { fetchCorrelation, fetchSymbols } from '../api/endpoints'
import type { Interval } from '../api/types'

export interface CorrelationMatrixProps {
  interval: Interval
}

const DEFAULT_SYMBOLS = ['BTCUSDT', 'ETHUSDT', 'SOLUSDT']
/** `ApiLimits.max_correlation_symbols` on the backend (crates/api/src/state.rs). */
const MAX_SYMBOLS = 10

function correlationColor(value: number | null): string {
  if (value === null) return 'var(--color-surface-2)'
  const token = value >= 0 ? '--color-up' : '--color-down'
  const pct = Math.round(Math.abs(value) * 100)
  return `color-mix(in srgb, var(${token}) ${pct}%, var(--color-surface))`
}

/** FR-6.1/6.2/6.3: pairwise correlation heatmap. Diverging scale (0 =
 * neutral), and the number is always shown in the cell too — color is
 * never the only carrier of the value (FR-6.3/NFR-3.3). */
export function CorrelationMatrix(props: CorrelationMatrixProps) {
  const [selected, setSelected] = createSignal<string[]>(DEFAULT_SYMBOLS)
  const [filter, setFilter] = createSignal('')

  const symbolsQuery = createQuery(() => ({
    queryKey: ['symbols'],
    queryFn: ({ signal }) => fetchSymbols({ signal }),
    staleTime: Infinity,
  }))

  const correlationQuery = createQuery(() => ({
    queryKey: ['correlation', selected().slice().sort().join(','), props.interval],
    queryFn: ({ signal }) => fetchCorrelation({ symbols: selected(), interval: props.interval }, { signal }),
    enabled: selected().length >= 2,
  }))

  const matrix = createMemo(() => {
    const pairs = correlationQuery.data ?? []
    const lookup = new Map<string, number | null>()
    for (const p of pairs) {
      lookup.set(`${p.symbolA}|${p.symbolB}`, p.correlation)
      lookup.set(`${p.symbolB}|${p.symbolA}`, p.correlation)
    }
    return lookup
  })

  const suggestions = createMemo(() => {
    const needle = filter().trim().toUpperCase()
    const all = symbolsQuery.data ?? []
    return all
      .map((s) => s.symbol)
      .filter((s) => !selected().includes(s))
      .filter((s) => needle.length === 0 || s.includes(needle))
      .slice(0, 8)
  })

  function addSymbol(symbol: string) {
    if (selected().length >= MAX_SYMBOLS || selected().includes(symbol)) return
    setSelected((prev) => [...prev, symbol])
    setFilter('')
  }

  function removeSymbol(symbol: string) {
    setSelected((prev) => prev.filter((s) => s !== symbol))
  }

  return (
    <div class="rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] p-2">
      <div class="mb-2 flex items-center justify-between">
        <span class="text-xs text-[var(--color-fg-muted)]">Correlation Matrix</span>
        <span class="text-xs text-[var(--color-fg-muted)]">
          {selected().length}/{MAX_SYMBOLS}
        </span>
      </div>

      <div class="mb-2 flex flex-wrap items-center gap-1">
        <For each={selected()}>
          {(symbol) => (
            <button
              type="button"
              onClick={() => removeSymbol(symbol)}
              class="rounded-full bg-[var(--color-surface-2)] px-2 py-0.5 text-xs text-[var(--color-fg)] hover:bg-[var(--color-border)]"
              title="Убрать"
            >
              {symbol} ×
            </button>
          )}
        </For>
        <Show when={selected().length < MAX_SYMBOLS}>
          <div class="relative">
            <input
              type="text"
              value={filter()}
              onInput={(e) => setFilter(e.currentTarget.value)}
              placeholder="+ символ"
              class="w-24 rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-2 py-0.5 text-xs text-[var(--color-fg)] outline-none focus:border-[var(--color-accent)]"
            />
            <Show when={filter().length > 0 && suggestions().length > 0}>
              <ul class="absolute z-10 mt-1 max-h-40 w-32 overflow-y-auto rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] shadow-lg">
                <For each={suggestions()}>
                  {(symbol) => (
                    <li>
                      <button
                        type="button"
                        onMouseDown={() => addSymbol(symbol)}
                        class="block w-full px-2 py-1 text-left text-xs text-[var(--color-fg)] hover:bg-[var(--color-surface)]"
                      >
                        {symbol}
                      </button>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        </Show>
      </div>

      <Show
        when={selected().length >= 2}
        fallback={<div class="text-xs text-[var(--color-fg-muted)]">Нужно минимум 2 символа</div>}
      >
        <div class="overflow-x-auto">
          <table class="tabular border-collapse text-xs">
            <thead>
              <tr>
                <th class="p-1" />
                <For each={selected()}>{(s) => <th class="p-1 text-[var(--color-fg-muted)]">{s}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <For each={selected()}>
                {(rowSymbol) => (
                  <tr>
                    <th class="p-1 text-right text-[var(--color-fg-muted)]">{rowSymbol}</th>
                    <For each={selected()}>
                      {(colSymbol) => {
                        const value = () =>
                          rowSymbol === colSymbol ? 1 : (matrix().get(`${rowSymbol}|${colSymbol}`) ?? null)
                        return (
                          <td
                            class="min-w-14 border border-[var(--color-bg)] p-1 text-center text-[var(--color-fg)]"
                            style={{ 'background-color': correlationColor(value()) }}
                          >
                            {value() === null ? '—' : value()!.toFixed(2)}
                          </td>
                        )
                      }}
                    </For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
    </div>
  )
}

// `solid-js`'s `lazy()` (used in app.tsx to keep this out of the initial
// bundle — it's off by default) needs a default export.
export default CorrelationMatrix
