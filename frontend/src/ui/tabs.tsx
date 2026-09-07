import { For, type JSX } from 'solid-js'

export interface TabsProps {
  tabs: readonly string[]
  active: string
  onChange: (tab: string) => void
  children: JSX.Element
}

export function Tabs(props: TabsProps) {
  return (
    <div class="flex h-full flex-col">
      <div class="flex border-b border-[var(--color-border)]">
        <For each={props.tabs}>
          {(tab) => (
            <button
              type="button"
              onClick={() => props.onChange(tab)}
              class="flex-1 border-b-2 px-2 py-1.5 text-xs"
              classList={{
                'border-[var(--color-accent)] text-[var(--color-fg)]': tab === props.active,
                'border-transparent text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]':
                  tab !== props.active,
              }}
            >
              {tab}
            </button>
          )}
        </For>
      </div>
      <div class="min-h-0 flex-1">{props.children}</div>
    </div>
  )
}
