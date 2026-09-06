/** Reads a design token (`tokens.css`, DR-2) as a resolved color string,
 * for the chart libraries (Lightweight Charts, uPlot) that need real CSS
 * color values up front rather than a stylesheet class. */
export function cssToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}
