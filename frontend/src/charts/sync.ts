import { createSignal } from 'solid-js'

export interface VisibleRange {
  from: number
  to: number
}

const [visibleRange, setVisibleRangeSignal] = createSignal<VisibleRange | null>(null)
const [origin, setOrigin] = createSignal<symbol | null>(null)

export { visibleRange, origin }

export function publishVisibleRange(source: symbol, range: VisibleRange): void {
  setOrigin(source)
  setVisibleRangeSignal(range)
}

const USER_GESTURE_WINDOW_MS = 150

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
