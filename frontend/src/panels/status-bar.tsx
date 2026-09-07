import { createMemo } from 'solid-js'
import { compute24hStats } from './stats'
import { ConnectionStatus } from '../ui/connection-status'
import type { ConnectionState } from '../stream/socket'
import type { OhlcvRow } from '../api/types'

export interface StatusBarProps {
  symbol: string
  rows: OhlcvRow[]
  liveKline: OhlcvRow | null
  connectionState: ConnectionState
}

function formatUpdateTime(ts: number | null): string {
  if (ts === null) return '—'
  return new Date(ts).toLocaleTimeString('ru-RU', { hour12: false })
}

export function StatusBar(props: StatusBarProps) {
  const currentPrice = createMemo(() => props.liveKline?.close ?? props.rows.at(-1)?.close ?? null)
  const lastUpdateTs = createMemo(() => props.liveKline?.closeTime ?? props.rows.at(-1)?.closeTime ?? null)
  const stats = createMemo(() => compute24hStats(props.rows, currentPrice(), Date.now()))

  return (
    <div class="tabular flex items-center gap-4 border-b border-[var(--color-border)] bg-[var(--color-surface)] px-4 py-1.5 text-xs">
      <span class="font-semibold text-[var(--color-fg)]">{props.symbol}</span>

      <span class="text-[var(--color-fg)]">{currentPrice()?.toFixed(2) ?? '—'}</span>

      <span
        classList={{
          'text-[var(--color-up)]': (stats().changePercent ?? 0) >= 0,
          'text-[var(--color-down)]': (stats().changePercent ?? 0) < 0,
          'text-[var(--color-fg-muted)]': stats().changePercent === null,
        }}
      >
        {stats().changePercent === null
          ? '24ч —'
          : `${stats().changePercent! >= 0 ? '+' : ''}${stats().changePercent!.toFixed(2)}% 24ч`}
      </span>

      <span class="text-[var(--color-fg-muted)]">Vol 24ч: {stats().volume.toFixed(2)}</span>

      <span class="ml-auto text-[var(--color-fg-muted)]">Обновлено: {formatUpdateTime(lastUpdateTs())}</span>

      <ConnectionStatus state={props.connectionState} />
    </div>
  )
}
