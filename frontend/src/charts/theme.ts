import { createMemo } from 'solid-js'
import { colorblindPalette, theme } from '../state/theme'

export function cssToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

export function themedCssToken(name: string): () => string {
  const value = createMemo(() => {
    theme()
    colorblindPalette()
    return cssToken(name)
  })
  return value
}
