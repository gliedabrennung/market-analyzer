import type { ConnectionState } from '../stream/socket'

export interface ConnectionStatusProps {
  state: ConnectionState
}

const LABELS: Record<ConnectionState, string> = {
  connecting: 'Подключение…',
  live: 'Live',
  reconnecting: 'Переподключение…',
  offline: 'Офлайн',
}

export function ConnectionStatus(props: ConnectionStatusProps) {
  return (
    <div class="flex items-center gap-1.5 text-xs text-[var(--color-fg-muted)]">
      <span
        class="h-2 w-2 rounded-full"
        classList={{
          'bg-[var(--color-up)]': props.state === 'live',
          'bg-[var(--color-fg-muted)] animate-pulse':
            props.state === 'connecting' || props.state === 'reconnecting',
          'bg-[var(--color-down)]': props.state === 'offline',
        }}
      />
      {LABELS[props.state]}
    </div>
  )
}
