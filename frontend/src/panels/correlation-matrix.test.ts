import { describe, expect, it } from 'vitest'
import { correlationColor } from './correlation-matrix'

describe('correlationColor', () => {
  it('is the neutral surface token for null (no overlapping data)', () => {
    expect(correlationColor(null)).toBe('var(--color-surface-2)')
  })

  it('blends towards --color-up for positive correlation, capped at 50%', () => {
    expect(correlationColor(1)).toBe('color-mix(in srgb, var(--color-up) 50%, var(--color-surface))')
    expect(correlationColor(0.5)).toBe('color-mix(in srgb, var(--color-up) 25%, var(--color-surface))')
    expect(correlationColor(0)).toBe('color-mix(in srgb, var(--color-up) 0%, var(--color-surface))')
  })

  it('blends towards --color-down for negative correlation, capped at 50%', () => {
    expect(correlationColor(-1)).toBe('color-mix(in srgb, var(--color-down) 50%, var(--color-surface))')
    expect(correlationColor(-0.5)).toBe('color-mix(in srgb, var(--color-down) 25%, var(--color-surface))')
  })
})
