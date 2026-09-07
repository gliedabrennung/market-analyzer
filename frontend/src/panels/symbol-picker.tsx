import { createEffect, createMemo, createSignal, For, Show } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { fetchSymbols } from '../api/endpoints'
import { recentSymbols, pushRecentSymbol } from '../state/recent-symbols'

/** The list is scrollable (`max-h-64`), so this only bounds how much DOM
 * one keystroke rebuilds — not what the user can reach, which is what
 * typing is for. */
const MAX_OPTIONS = 50

export interface SymbolPickerProps {
  value: string
  onSelect: (symbol: string) => void
}

/** FR-8.1/NFR-3.1: filter the `/symbols` registry, arrow-key/Enter/Escape
 * navigation, recently-used symbols shown first when the filter is empty. */
export function SymbolPicker(props: SymbolPickerProps) {
  const [filter, setFilter] = createSignal('')
  const [open, setOpen] = createSignal(false)
  const [activeIndex, setActiveIndex] = createSignal(0)
  let inputRef: HTMLInputElement | undefined

  const symbolsQuery = createQuery(() => ({
    queryKey: ['symbols'],
    queryFn: ({ signal }) => fetchSymbols({ signal }),
    staleTime: Infinity,
  }))

  /** The registry is the exchange's entire pair list: ~3700 entries, most
   * of them long delisted (`status` other than `TRADING`), and only the
   * handful that were actually collected can draw a chart. Listing it raw
   * and alphabetically meant the dropdown opened on `0GBNB, 1000CATBNB,
   * 1INCHDOWNUSDT…` — nothing a person would pick — and every second pick
   * landed on a symbol with no data, which looks exactly like a broken
   * app. Delisted pairs are dropped, collected ones come first, and the
   * rest stay reachable by typing. */
  const ranked = createMemo(() => {
    const tradable = (symbolsQuery.data ?? []).filter((s) => s.status === 'TRADING')
    const withData = tradable.filter((s) => s.hasData).map((s) => s.symbol)
    const withoutData = tradable.filter((s) => !s.hasData).map((s) => s.symbol)
    return { withData, withoutData }
  })

  const options = createMemo(() => {
    const needle = filter().trim().toUpperCase()
    const { withData, withoutData } = ranked()
    const match = (s: string) => needle.length === 0 || s.includes(needle)

    // Recents first (they are what a person actually switches between),
    // then everything holding data, then the rest of the tradable list.
    const recents = recentSymbols().filter(
      (s) => match(s) && (withData.includes(s) || withoutData.includes(s)),
    )
    const seen = new Set(recents)
    const rest = [...withData, ...withoutData].filter((s) => match(s) && !seen.has(s))
    return [...recents, ...rest].slice(0, MAX_OPTIONS)
  })

  const hasData = createMemo(() => new Set(ranked().withData))

  // Keep the highlighted row in range whenever the option list changes.
  createEffect(() => {
    if (activeIndex() >= options().length) setActiveIndex(Math.max(0, options().length - 1))
  })

  function select(symbol: string) {
    props.onSelect(symbol)
    pushRecentSymbol(symbol)
    setFilter('')
    setOpen(false)
    inputRef?.blur()
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setOpen(true)
      setActiveIndex((i) => Math.min(i + 1, options().length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActiveIndex((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const symbol = options()[activeIndex()]
      if (symbol !== undefined) select(symbol)
    } else if (e.key === 'Escape') {
      setOpen(false)
      inputRef?.blur()
    }
  }

  return (
    <div class="relative">
      <input
        ref={inputRef}
        data-hotkey-target="symbol-search"
        type="text"
        role="combobox"
        aria-expanded={open()}
        aria-controls="symbol-picker-listbox"
        aria-autocomplete="list"
        class="w-40 rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-2 py-1 text-sm text-[var(--color-fg)] outline-none focus:border-[var(--color-accent)]"
        placeholder={props.value}
        value={filter()}
        onInput={(e) => {
          setFilter(e.currentTarget.value)
          setActiveIndex(0)
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onKeyDown={onKeyDown}
      />
      <Show when={open()}>
        <ul
          id="symbol-picker-listbox"
          role="listbox"
          class="absolute z-10 mt-1 max-h-64 w-48 overflow-y-auto rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] shadow-lg"
        >
          <Show
            when={!symbolsQuery.isLoading}
            fallback={<li class="px-2 py-1 text-sm text-[var(--color-fg-muted)]">Загрузка…</li>}
          >
            <For
              each={options()}
              fallback={<li class="px-2 py-1 text-sm text-[var(--color-fg-muted)]">Ничего не найдено</li>}
            >
              {(symbol, index) => (
                <li role="option" aria-selected={index() === activeIndex()}>
                  <button
                    type="button"
                    class="block w-full px-2 py-1 text-left text-sm"
                    classList={{
                      'bg-[var(--color-accent)] text-white': index() === activeIndex(),
                      'text-[var(--color-fg)] hover:bg-[var(--color-surface)]': index() !== activeIndex(),
                    }}
                    onMouseEnter={() => setActiveIndex(index())}
                    onMouseDown={() => select(symbol)}
                  >
                    <span class="flex items-center justify-between gap-2">
                      <span>{symbol}</span>
                      <Show when={!hasData().has(symbol)}>
                        <span
                          class="text-xs"
                          classList={{
                            'text-white': index() === activeIndex(),
                            'text-[var(--color-fg-muted)]': index() !== activeIndex(),
                          }}
                          title="Данные по этой паре ещё не загружены"
                        >
                          нет данных
                        </span>
                      </Show>
                    </span>
                  </button>
                </li>
              )}
            </For>
          </Show>
        </ul>
      </Show>
    </div>
  )
}
