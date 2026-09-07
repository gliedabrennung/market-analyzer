import { createEffect, createSignal, For, Show } from 'solid-js'
import { createVirtualizer } from '@tanstack/solid-virtual'
import type { LiveTrade } from '../stream/socket'

export interface TradeTapeProps {
  trades: LiveTrade[]
}

const ROW_HEIGHT = 22

function formatTime(ts: number): string {
  const d = new Date(ts)
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  const ss = String(d.getSeconds()).padStart(2, '0')
  const ms = String(d.getMilliseconds()).padStart(3, '0')
  return `${hh}:${mm}:${ss}.${ms}`
}

/** FR-7.1/7.2/7.3: virtualized trade tape — newest first, ≤500 in memory
 * (capped upstream by `useLiveStream`), only visible rows in the DOM.
 * Autoscrolls to new trades unless the user has scrolled down to look at
 * older ones, in which case a "jump to latest" button resumes it — per
 * FR-7.3, only the button re-enables autoscroll, not scrolling back up
 * manually. */
export function TradeTape(props: TradeTapeProps) {
  // Newest-first for display; `props.trades` itself stays chronological
  // (oldest→newest) end-to-end elsewhere in the app.
  const rows = () => props.trades.slice().reverse()

  const [autoScroll, setAutoScroll] = createSignal(true)
  let scrollParent: HTMLDivElement | undefined
  let ignoreNextScrollEvent = false

  const virtualizer = createVirtualizer({
    get count() {
      return rows().length
    },
    getScrollElement: () => scrollParent ?? null,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  })

  function scrollToTop() {
    if (!scrollParent) return
    ignoreNextScrollEvent = true
    scrollParent.scrollTop = 0
  }

  function handleScroll() {
    if (ignoreNextScrollEvent) {
      ignoreNextScrollEvent = false
      return
    }
    if (scrollParent !== undefined && scrollParent.scrollTop > 4 && autoScroll()) {
      setAutoScroll(false)
    }
  }

  function resume() {
    setAutoScroll(true)
    scrollToTop()
  }

  createEffect(() => {
    rows() // track new trades arriving
    if (autoScroll()) scrollToTop()
  })

  return (
    <div class="relative flex h-full flex-col">
      <div class="grid grid-cols-[1fr_1fr_1fr_auto] gap-2 border-b border-[var(--color-border)] px-2 py-1 text-xs text-[var(--color-fg-muted)]">
        <span>Время</span>
        <span class="text-right">Цена</span>
        <span class="text-right">Объём</span>
        <span class="w-6 text-right">Сторона</span>
      </div>

      <div ref={scrollParent} onScroll={handleScroll} class="min-h-0 flex-1 overflow-y-auto">
        <div class="relative w-full" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          <For each={virtualizer.getVirtualItems()}>
            {(virtualRow) => {
              const trade = () => rows()[virtualRow.index]
              return (
                <div
                  class="tabular absolute top-0 left-0 grid w-full grid-cols-[1fr_1fr_1fr_auto] gap-2 px-2 text-xs"
                  style={{
                    height: `${virtualRow.size}px`,
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                >
                  <span class="text-[var(--color-fg-muted)]">{formatTime(trade()?.ts ?? 0)}</span>
                  <span class="text-right text-[var(--color-fg)]">{trade()?.price.toFixed(2)}</span>
                  <span class="text-right text-[var(--color-fg-muted)]">{trade()?.qty.toFixed(5)}</span>
                  <span
                    class="w-6 text-right font-semibold"
                    classList={{
                      'text-[var(--color-down)]': trade()?.isBuyerMaker,
                      'text-[var(--color-up)]': trade()?.isBuyerMaker === false,
                    }}
                  >
                    {trade()?.isBuyerMaker ? 'S' : 'B'}
                  </span>
                </div>
              )
            }}
          </For>
        </div>
      </div>

      <Show when={!autoScroll()}>
        <button
          type="button"
          onClick={resume}
          class="absolute bottom-2 left-1/2 -translate-x-1/2 rounded-full bg-[var(--color-accent)] px-3 py-1 text-xs text-white shadow-lg"
        >
          ↑ новые сделки
        </button>
      </Show>
    </div>
  )
}

// `solid-js`'s `lazy()` (app.tsx) needs a default export. Lazy despite
// being the default-shown sidebar tab: it starts empty regardless (no
// history endpoint for trades, only the live feed), so a brief chunk-load
// delay here is faster than the wait for the first real trade anyway —
// worth it to keep `@tanstack/solid-virtual` out of the critical bundle
// (NFR-1.1's initial-chunk budget).
export default TradeTape
