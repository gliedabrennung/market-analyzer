export function ChartSkeleton() {
  return (
    <div class="flex h-full w-full animate-pulse items-center justify-center bg-[var(--color-surface)]">
      <div class="h-2/3 w-[95%] rounded-md bg-[var(--color-surface-2)]" />
    </div>
  )
}

export function PanelSkeleton() {
  return (
    <div class="h-40 animate-pulse rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] p-2">
      <div class="h-full w-full rounded-md bg-[var(--color-surface-2)]" />
    </div>
  )
}
