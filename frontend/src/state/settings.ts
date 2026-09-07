import { createEffect, createRoot, createSignal } from 'solid-js'

export interface IndicatorSettings {
  vwap: boolean
  ma: boolean
  volatility: boolean
  ofi: boolean
  anomalies: boolean
  correlation: boolean
}

const STORAGE_KEY = 'market-analyzer:indicators'

const DEFAULTS: IndicatorSettings = {
  vwap: true,
  ma: true,
  volatility: true,

  ofi: false,
  anomalies: true,

  correlation: false,
}

function loadInitial(): IndicatorSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === null) return DEFAULTS
    return { ...DEFAULTS, ...(JSON.parse(raw) as Partial<IndicatorSettings>) }
  } catch {
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
    } catch {}
  })
})
