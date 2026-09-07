/** FR-2.3: batches values pushed via `push()`, flushing at most once per
 * animation frame via `onFlush` — the actual point of this module: at
 * 1000 msg/sec (NFR-1.5) applying every message individually would mean
 * 1000 UI updates/sec, not 60.
 *
 * FR-2.4: if more than `maxBuffered` items pile up between frames (a
 * busy/janky tab skipping frames under load), keeps only the newest
 * `keepOnOverflow` before flushing — degrades update *frequency*, not
 * responsiveness. What "keep" means is caller-defined: the trade tape
 * caller passes its own display cap (works either way, this list is
 * append-only from the caller's side); the live-kline caller passes `1`
 * (only the newest kline state is ever meaningful — older ones in the
 * same tick are already-superseded versions of the same or an earlier
 * bar).
 */
export interface RafBufferOptions<T> {
  onFlush: (items: T[]) => void
  maxBuffered?: number
  keepOnOverflow?: number
}

export interface RafBuffer<T> {
  push: (item: T) => void
  stop: () => void
}

const DEFAULT_MAX_BUFFERED = 5000

export function createRafBuffer<T>(options: RafBufferOptions<T>): RafBuffer<T> {
  const maxBuffered = options.maxBuffered ?? DEFAULT_MAX_BUFFERED
  const keepOnOverflow = options.keepOnOverflow ?? maxBuffered
  let queue: T[] = []
  let frameHandle: number | null = null

  function scheduleFlush() {
    if (frameHandle !== null) return
    frameHandle = requestAnimationFrame(() => {
      frameHandle = null
      if (queue.length === 0) return
      const items = queue
      queue = []
      options.onFlush(items)
    })
  }

  return {
    push(item) {
      queue.push(item)
      if (queue.length > maxBuffered) {
        queue = queue.slice(-keepOnOverflow)
      }
      scheduleFlush()
    },
    stop() {
      if (frameHandle !== null) {
        cancelAnimationFrame(frameHandle)
        frameHandle = null
      }
      queue = []
    },
  }
}
