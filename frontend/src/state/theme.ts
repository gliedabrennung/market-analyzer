import { createSignal } from 'solid-js'

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

applyThemeAttr(initialTheme)
applyPaletteAttr(initialColorblind)

export const [theme, setThemeSignal] = createSignal<Theme>(initialTheme)
export const [colorblindPalette, setColorblindSignal] = createSignal<boolean>(initialColorblind)

export function setTheme(value: Theme): void {
  applyThemeAttr(value)
  try {
    localStorage.setItem(THEME_KEY, value)
  } catch {}
  setThemeSignal(value)
}

export function toggleTheme(): void {
  setTheme(theme() === 'dark' ? 'light' : 'dark')
}

export function setColorblindPalette(value: boolean): void {
  applyPaletteAttr(value)
  try {
    localStorage.setItem(COLORBLIND_KEY, value ? '1' : '0')
  } catch {}
  setColorblindSignal(value)
}

export function toggleColorblindPalette(): void {
  setColorblindPalette(!colorblindPalette())
}
