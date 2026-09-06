import { createEffect, createRoot, createSignal } from 'solid-js'

/** FR-4.3: which overlays/panels are on. FR-8.4: persisted in
 * `localStorage` so the choice survives a reload. */
export interface IndicatorSettings {
  vwap: boolean
  ma: boolean
  volatility: boolean
  ofi: boolean
  anomalies: boolean
}

const STORAGE_KEY = 'market-analyzer:indicators'

const DEFAULTS: IndicatorSettings = {
  vwap: true,
  ma: true,
  volatility: true,
  // OFI needs the `trades` dataset (not just klines) and is only really
  // informative with a live/near-live feed — off by default so a fresh
  // visit doesn't fetch it against symbols with little trade history.
  ofi: false,
  anomalies: true,
}

function loadInitial(): IndicatorSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === null) return DEFAULTS
    return { ...DEFAULTS, ...(JSON.parse(raw) as Partial<IndicatorSettings>) }
  } catch {
    // Private-mode storage access, corrupted JSON, etc. — fall back to
    // defaults rather than fail the whole app over a settings read.
    return DEFAULTS
  }
}

export const [indicatorSettings, setIndicatorSettings] = createSignal<IndicatorSettings>(loadInitial())

export function toggleIndicator(key: keyof IndicatorSettings): void {
  setIndicatorSettings((prev) => ({ ...prev, [key]: !prev[key] }))
}

createRoot(() => {
  createEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(indicatorSettings()))
    } catch {
      // Quota exceeded / private mode — losing the persisted choice isn't
      // worth failing anything else over.
    }
  })
})
