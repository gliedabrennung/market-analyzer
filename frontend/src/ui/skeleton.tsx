/** DR-6: a shape of the content to come, not a spinner — avoids layout
 * shift when real content arrives (NFR-1.6). */
export function ChartSkeleton() {
  return (
    <div class="flex h-full w-full animate-pulse items-center justify-center bg-[var(--color-surface)]">
      <div class="h-2/3 w-[95%] rounded-md bg-[var(--color-surface-2)]" />
    </div>
  )
}
