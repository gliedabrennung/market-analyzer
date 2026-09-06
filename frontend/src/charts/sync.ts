import { createSignal } from 'solid-js'

/** Unix seconds, matching Lightweight Charts' `Time`/uPlot's x-scale unit. */
export interface VisibleRange {
  from: number
  to: number
}

/** FR-4.2: one shared "currently visible time window", written by
 * whichever chart the user is panning/zooming and read by every other
 * chart/panel to follow along. `origin` names which chart wrote the
 * current value, so that same chart can skip re-applying its own change
 * (avoiding feedback loops and interrupting an in-flight gesture) while
 * every *other* chart still reacts to it.
 *
 * A plain shared signal rather than each chart subscribing to every other
 * chart directly — panels are added/removed independently (FR-4.3), so
 * there's no fixed set of peers to wire up pairwise.
 */
const [visibleRange, setVisibleRangeSignal] = createSignal<VisibleRange | null>(null)
const [origin, setOrigin] = createSignal<symbol | null>(null)

export { visibleRange, origin }

export function publishVisibleRange(source: symbol, range: VisibleRange): void {
  setOrigin(source)
  setVisibleRangeSignal(range)
}

/** How long after a real pointer/wheel event on a chart's own element a
 * range change from that chart still counts as "the user did this".
 * Covers a drag's whole duration (each `pointermove` refreshes it) and a
 * single wheel-zoom tick. */
const USER_GESTURE_WINDOW_MS = 150

/** Both Lightweight Charts and uPlot fire their "visible range changed"
 * callback for *any* cause — a real drag/zoom, but just as much a
 * `setData()`-triggered auto-rescale or the chart's own initial
 * auto-fit. Only the first should ever be published to `./sync` — the
 * others aren't the user expressing an intended view, and publishing them
 * anyway means whichever chart/panel finishes loading (or mounting) last
 * silently clobbers every other chart's viewport (observed: an indicator
 * panel's own construction-time auto-scale was overwriting the main
 * chart's `fitContent()` view moments after it correctly rendered).
 *
 * There's no library-level way to ask "was this change a user gesture" —
 * this tracks real DOM input events on the chart's own root element as a
 * proxy instead, which works identically for both chart libraries.
 * Returns a `wasRecentUserGesture()` check and a cleanup function.
 */
export function trackUserGestures(element: HTMLElement): {
  wasRecentUserGesture: () => boolean
  stop: () => void
} {
  let lastInputAt = 0
  const markInput = () => {
    lastInputAt = performance.now()
  }
  element.addEventListener('pointerdown', markInput)
  element.addEventListener('pointermove', markInput)
  element.addEventListener('wheel', markInput, { passive: true })

  return {
    wasRecentUserGesture: () => performance.now() - lastInputAt < USER_GESTURE_WINDOW_MS,
    stop: () => {
      element.removeEventListener('pointerdown', markInput)
      element.removeEventListener('pointermove', markInput)
      element.removeEventListener('wheel', markInput)
    },
  }
}
