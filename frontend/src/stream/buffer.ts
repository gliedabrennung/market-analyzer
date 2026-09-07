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
