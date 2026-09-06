import { For } from 'solid-js'
import { INTERVALS, type Interval } from '../api/types'

export interface IntervalSwitcherProps {
  value: Interval
  onChange: (interval: Interval) => void
}

export function IntervalSwitcher(props: IntervalSwitcherProps) {
  return (
    <div class="flex gap-1">
      <For each={INTERVALS}>
        {(interval) => (
          <button
            type="button"
            class="rounded-md px-2 py-1 text-sm"
            classList={{
              'bg-[var(--color-accent)] text-white': interval === props.value,
              'text-[var(--color-fg-muted)] hover:bg-[var(--color-surface-2)]': interval !== props.value,
            }}
            onClick={() => props.onChange(interval)}
          >
            {interval}
          </button>
        )}
      </For>
    </div>
  )
}
