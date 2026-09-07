import { createSignal } from 'solid-js'

/** FR-8.1: "недавно использованные наверху". */
const STORAGE_KEY = 'market-analyzer:recent-symbols'
const MAX_RECENT = 6

function loadInitial(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === null) return []
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((s): s is string => typeof s === 'string') : []
  } catch {
    return []
  }
}

export const [recentSymbols, setRecentSymbols] = createSignal<string[]>(loadInitial())

export function pushRecentSymbol(symbol: string): void {
  setRecentSymbols((prev) => {
    const next = [symbol, ...prev.filter((s) => s !== symbol)].slice(0, MAX_RECENT)
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
    } catch {
      // best-effort persistence; losing the recents list isn't worth failing over
    }
    return next
  })
}
