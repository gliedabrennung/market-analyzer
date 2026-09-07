import { createSignal } from 'solid-js'

/** DR-1/FR-8.4: dark by default, light is opt-in and persisted. */
export type Theme = 'dark' | 'light'

const THEME_KEY = 'market-analyzer:theme'
const COLORBLIND_KEY = 'market-analyzer:colorblind-palette'

function loadTheme(): Theme {
  try {
    return localStorage.getItem(THEME_KEY) === 'light' ? 'light' : 'dark'
  } catch {
    return 'dark'
  }
}

function loadColorblind(): boolean {
  try {
    return localStorage.getItem(COLORBLIND_KEY) === '1'
  } catch {
    return false
  }
}

function applyThemeAttr(value: Theme): void {
  document.documentElement.dataset.theme = value
}

function applyPaletteAttr(value: boolean): void {
  if (value) {
    document.documentElement.dataset.palette = 'colorblind'
  } else {
    delete document.documentElement.dataset.palette
  }
}

const initialTheme = loadTheme()
const initialColorblind = loadColorblind()
// Applied synchronously at module load, before any chart's `onMount`
// first reads a color token (`charts/theme.ts`'s `cssToken`) — otherwise
// the very first frame would paint in the wrong palette and then jump.
applyThemeAttr(initialTheme)
applyPaletteAttr(initialColorblind)

export const [theme, setThemeSignal] = createSignal<Theme>(initialTheme)
export const [colorblindPalette, setColorblindSignal] = createSignal<boolean>(initialColorblind)

/** Deliberately imperative, not a `createEffect` on `theme`. PriceChart's
 * and IndicatorPanel's own theme-change effects re-read resolved colors
 * via `getComputedStyle` when *they* run; if the `data-theme` attribute
 * were updated by a same-signal effect instead, its ordering relative to
 * theirs would be unspecified — both would be direct subscribers of the
 * same signal, not parent/child. Setting the attribute inline, in the
 * same call that changes the signal, guarantees it lands before any
 * dependent effect anywhere gets a chance to run. */
export function setTheme(value: Theme): void {
  applyThemeAttr(value)
  try {
    localStorage.setItem(THEME_KEY, value)
  } catch {
    // Private mode / quota — losing the persisted choice isn't worth
    // failing anything else over.
  }
  setThemeSignal(value)
}

export function toggleTheme(): void {
  setTheme(theme() === 'dark' ? 'light' : 'dark')
}

/** Same imperative reasoning as `setTheme`. */
export function setColorblindPalette(value: boolean): void {
  applyPaletteAttr(value)
  try {
    localStorage.setItem(COLORBLIND_KEY, value ? '1' : '0')
  } catch {
    // See setTheme.
  }
  setColorblindSignal(value)
}

export function toggleColorblindPalette(): void {
  setColorblindPalette(!colorblindPalette())
}
