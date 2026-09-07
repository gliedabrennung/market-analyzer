import { createEffect, createMemo, createSignal, For, onCleanup } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { fetchAnomalies } from '../api/endpoints'
import type { Interval } from '../api/types'

export interface AnomaliesTableProps {
  symbol: string
  interval: Interval

  priceByTime: Map<number, number>
  onSelect: (openTime: number) => void
}

const DEBOUNCE_MS = 300
const DEFAULT_THRESHOLD = 3.0
const DEFAULT_WINDOW = 100
const MAX_ROWS_SHOWN = 200

type SortKey = 'time' | 'zscore'

function formatTime(ts: number): string {
  return new Date(ts).toLocaleString('ru-RU', { dateStyle: 'short', timeStyle: 'medium' })
}

export function AnomaliesTable(props: AnomaliesTableProps) {
  const [thresholdInput, setThresholdInput] = createSignal(DEFAULT_THRESHOLD)
  const [windowInput, setWindowInput] = createSignal(DEFAULT_WINDOW)
  const [threshold, setThreshold] = createSignal(DEFAULT_THRESHOLD)
  const [windowSize, setWindowSize] = createSignal(DEFAULT_WINDOW)
  const [sortKey, setSortKey] = createSignal<SortKey>('time')
  const [sortDesc, setSortDesc] = createSignal(true)

  let debounceTimer: number | undefined
  createEffect(() => {
    const t = thresholdInput()
    const w = windowInput()
    if (debounceTimer !== undefined) window.clearTimeout(debounceTimer)
    debounceTimer = window.setTimeout(() => {
      setThreshold(t)
      setWindowSize(w)
    }, DEBOUNCE_MS)
  })
  onCleanup(() => {
    if (debounceTimer !== undefined) window.clearTimeout(debounceTimer)
  })

  const query = createQuery(() => ({
    queryKey: ['anomalies-table', props.symbol, props.interval, threshold(), windowSize()],
    queryFn: ({ signal }) =>
      fetchAnomalies(
        { symbol: props.symbol, interval: props.interval, threshold: threshold(), window: windowSize() },
        { signal },
      ),
  }))

  function toggleSort(key: SortKey) {
    if (sortKey() === key) {
      setSortDesc((prev) => !prev)
    } else {
      setSortKey(key)
      setSortDesc(true)
    }
  }

  const sortedRows = createMemo(() => {
    const rows = (query.data ?? []).slice()
    const key = sortKey()
    const dir = sortDesc() ? -1 : 1
    rows.sort((a, b) => {
      const av = key === 'time' ? a.openTime : a.zScore
      const bv = key === 'time' ? b.openTime : b.zScore
      return (av - bv) * dir
    })
    return rows.slice(0, MAX_ROWS_SHOWN)
  })

  function sortIndicator(key: SortKey): string {
    if (sortKey() !== key) return ''
    return sortDesc() ? ' ↓' : ' ↑'
  }

  return (
    <div class="flex h-full flex-col">
      <div class="flex items-center gap-3 border-b border-[var(--color-border)] px-2 py-1.5 text-xs">
        <label class="flex items-center gap-1 text-[var(--color-fg-muted)]">
          порог
          <input
            type="number"
            step="0.1"
            min="0.1"
            value={thresholdInput()}
            onInput={(e) => setThresholdInput(Number.parseFloat(e.currentTarget.value) || DEFAULT_THRESHOLD)}
            class="tabular w-14 rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-1 py-0.5 text-[var(--color-fg)] outline-none focus:border-[var(--color-accent)]"
          />
        </label>
        <label class="flex items-center gap-1 text-[var(--color-fg-muted)]">
          окно
          <input
            type="number"
            step="10"
            min="2"
            value={windowInput()}
            onInput={(e) => setWindowInput(Number.parseInt(e.currentTarget.value, 10) || DEFAULT_WINDOW)}
            class="tabular w-14 rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-1 py-0.5 text-[var(--color-fg)] outline-none focus:border-[var(--color-accent)]"
          />
        </label>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto">
        <table class="tabular w-full text-xs">
          <thead class="sticky top-0 bg-[var(--color-surface)]">
            <tr class="border-b border-[var(--color-border)] text-[var(--color-fg-muted)]">
              <th class="cursor-pointer p-1 text-left select-none" onClick={() => toggleSort('time')}>
                Время{sortIndicator('time')}
              </th>
              <th class="p-1 text-right">Цена</th>
              <th class="p-1 text-right">Объём</th>
              <th class="cursor-pointer p-1 text-right select-none" onClick={() => toggleSort('zscore')}>
                Z-score{sortIndicator('zscore')}
              </th>
            </tr>
          </thead>
          <tbody>
            <For
              each={sortedRows()}
              fallback={
                <tr>
                  <td colspan="4" class="p-2 text-center text-[var(--color-fg-muted)]">
                    {query.isLoading ? 'Загрузка…' : 'Нет аномалий'}
                  </td>
                </tr>
              }
            >
              {(anomaly) => (
                <tr
                  class="cursor-pointer border-b border-[var(--color-border)] hover:bg-[var(--color-surface-2)]"
                  onClick={() => props.onSelect(anomaly.openTime)}
                >
                  <td class="p-1 text-[var(--color-fg-muted)]">{formatTime(anomaly.openTime)}</td>
                  <td class="p-1 text-right text-[var(--color-fg)]">
                    {props.priceByTime.get(anomaly.openTime)?.toFixed(2) ?? '—'}
                  </td>
                  <td class="p-1 text-right text-[var(--color-fg)]">{anomaly.volume.toFixed(4)}</td>
                  <td class="p-1 text-right font-semibold text-[var(--color-down)]">
                    {anomaly.zScore.toFixed(2)}
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  )
}

export default AnomaliesTable
