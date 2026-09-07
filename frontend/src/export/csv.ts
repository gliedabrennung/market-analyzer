import type { OhlcvRow } from '../api/types'

const CSV_HEADER = 'open_time,open,high,low,close,volume'

export function buildOhlcvCsv(rows: readonly OhlcvRow[]): string {
  const lines = rows.map((r) =>
    [new Date(r.openTime).toISOString(), r.open, r.high, r.low, r.close, r.volume].join(','),
  )
  return [CSV_HEADER, ...lines].join('\n')
}

export function downloadCsv(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  URL.revokeObjectURL(url)
}
