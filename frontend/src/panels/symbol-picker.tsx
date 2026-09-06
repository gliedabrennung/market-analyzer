import { createSignal, For, Show } from 'solid-js'
import { createQuery } from '@tanstack/solid-query'
import { fetchSymbols } from '../api/endpoints'

export interface SymbolPickerProps {
  value: string
  onSelect: (symbol: string) => void
}

/** FR-8.1 (minimal slice for Этап 1): filter the `/symbols` registry and
 * pick one. Keyboard nav / recents / full combobox semantics are Этап 4
 * polish, not needed for the Этап 1 acceptance bar. */
export function SymbolPicker(props: SymbolPickerProps) {
  const [filter, setFilter] = createSignal('')
  const [open, setOpen] = createSignal(false)

  const symbolsQuery = createQuery(() => ({
    queryKey: ['symbols'],
    queryFn: ({ signal }) => fetchSymbols({ signal }),
    staleTime: Infinity,
  }))

  const filtered = () => {
    const needle = filter().trim().toUpperCase()
    const all = symbolsQuery.data ?? []
    if (needle.length === 0) return all.slice(0, 20)
    return all.filter((s) => s.symbol.includes(needle)).slice(0, 20)
  }

  return (
    <div class="relative">
      <input
        type="text"
        class="w-40 rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-2 py-1 text-sm text-[var(--color-fg)] outline-none focus:border-[var(--color-accent)]"
        placeholder={props.value}
        value={filter()}
        onInput={(e) => setFilter(e.currentTarget.value)}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
      />
      <Show when={open()}>
        <ul class="absolute z-10 mt-1 max-h-64 w-48 overflow-y-auto rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] shadow-lg">
          <Show
            when={!symbolsQuery.isLoading}
            fallback={<li class="px-2 py-1 text-sm text-[var(--color-fg-muted)]">Загрузка…</li>}
          >
            <For
              each={filtered()}
              fallback={<li class="px-2 py-1 text-sm text-[var(--color-fg-muted)]">Ничего не найдено</li>}
            >
              {(item) => (
                <li>
                  <button
                    type="button"
                    class="block w-full px-2 py-1 text-left text-sm text-[var(--color-fg)] hover:bg-[var(--color-surface)]"
                    onMouseDown={() => {
                      props.onSelect(item.symbol)
                      setFilter('')
                      setOpen(false)
                    }}
                  >
                    {item.symbol}
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
