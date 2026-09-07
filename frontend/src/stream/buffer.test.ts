import { describe, expect, it, vi } from 'vitest'
import { createRafBuffer } from './buffer'

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()))
}

describe('createRafBuffer', () => {
  it('batches multiple pushes within a frame into one flush', async () => {
    const onFlush = vi.fn()
    const buffer = createRafBuffer<number>({ onFlush })

    buffer.push(1)
    buffer.push(2)
    buffer.push(3)
    expect(onFlush).not.toHaveBeenCalled()

    await nextFrame()
    await nextFrame() // the buffer's own rAF callback runs on the frame after these pushes

    expect(onFlush).toHaveBeenCalledTimes(1)
    expect(onFlush).toHaveBeenCalledWith([1, 2, 3])
  })

  it('FR-2.4: keeps only the newest keepOnOverflow items once maxBuffered is exceeded', async () => {
    const onFlush = vi.fn()
    const buffer = createRafBuffer<number>({ onFlush, maxBuffered: 5, keepOnOverflow: 2 })

    for (let i = 0; i < 10; i++) buffer.push(i)
    await nextFrame()
    await nextFrame()

    expect(onFlush).toHaveBeenCalledTimes(1)
    expect(onFlush).toHaveBeenCalledWith([8, 9])
  })

  it('stop() cancels a pending flush and clears the queue', async () => {
    const onFlush = vi.fn()
    const buffer = createRafBuffer<number>({ onFlush })

    buffer.push(1)
    buffer.stop()
    await nextFrame()
    await nextFrame()

    expect(onFlush).not.toHaveBeenCalled()
  })

  it('a push after stop() schedules a fresh flush', async () => {
    const onFlush = vi.fn()
    const buffer = createRafBuffer<number>({ onFlush })

    buffer.push(1)
    buffer.stop()
    buffer.push(2)
    await nextFrame()
    await nextFrame()

    expect(onFlush).toHaveBeenCalledTimes(1)
    expect(onFlush).toHaveBeenCalledWith([2])
  })
})
