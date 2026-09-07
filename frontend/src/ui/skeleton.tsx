/** DR-6: a shape of the content to come, not a spinner — avoids layout
 * shift when real content arrives (NFR-1.6). */
export function ChartSkeleton() {
  return (
    <div class="flex h-full w-full animate-pulse items-center justify-center bg-[var(--color-surface)]">
      <div class="h-2/3 w-[95%] rounded-md bg-[var(--color-surface-2)]" />
    </div>
  )
}

/** Same idea as `ChartSkeleton`, sized for a smaller card-style panel —
 * used for `lazy()`-loaded panels (app.tsx's `<Suspense>`), where there's
 * a real network gap (the chunk itself) on top of the data fetch. */
export function PanelSkeleton() {
  return (
    <div class="h-40 animate-pulse rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] p-2">
      <div class="h-full w-full rounded-md bg-[var(--color-surface-2)]" />
    </div>
  )
}
