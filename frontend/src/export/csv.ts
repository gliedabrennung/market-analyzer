import type { OhlcvRow } from '../api/types'

const CSV_HEADER = 'open_time,open,high,low,close,volume'

/** Pure CSV formatter — kept separate from `downloadCsv` (browser-only
 * `Blob`/anchor download) so it's trivial to unit-test without a DOM.
 * `openTime` is written as ISO 8601, not the raw epoch-ms this app uses
 * internally, since a downloaded file is for a human/spreadsheet to read,
 * not another `Date.parse()` call. Same number-precision caveat as
 * `OhlcvRow` itself (frontend-tz.md §2.3, `api/types.ts`) applies here —
 * this is chart-grade precision, not a source for money arithmetic. */
export function buildOhlcvCsv(rows: readonly OhlcvRow[]): string {
  const lines = rows.map((r) =>
    [new Date(r.openTime).toISOString(), r.open, r.high, r.low, r.close, r.volume].join(','),
  )
  return [CSV_HEADER, ...lines].join('\n')
}

/** NFR-2.6 names "экспорт" explicitly as a module that should only load
 * via dynamic `import()` — see `app.tsx`'s `exportCsv`. Not that a CSV
 * formatter is heavy on its own; but keeping the download-triggering DOM
 * code below (which `buildOhlcvCsv` above deliberately has no need of, so
 * it stays testable without a DOM) out of the always-loaded bundle costs
 * nothing and matches the ТЗ's own stated pattern for this feature. */
export function downloadCsv(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  URL.revokeObjectURL(url)
}
