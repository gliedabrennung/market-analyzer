#!/usr/bin/env node
// NFR-1.1 (frontend-tz.md §6.1): "Начальный JS-чанк < 150 КБ gzip; общий
// вес критического пути < 250 КБ" — "проверка в CI". Run after `vite
// build`; gzips the actual dist/ assets (matches what a browser
// transfers, independent of rollup-plugin-visualizer's own per-module
// accounting, which double-counts modules that end up merged into one
// gzip stream).
//
// "Начальный" ("initial") is load-bearing once lazy()/dynamic import()
// exists (Этап 4's CorrelationMatrix): only the script(s)/stylesheet(s)
// dist/index.html actually references eagerly count toward this budget —
// a lazy chunk sitting unused in dist/assets/ until a user action
// requests it is exactly what NFR-2.6 asks for, not a budget violation.
// Parses index.html's own <script>/<link rel=stylesheet> tags rather
// than summing every file under dist/, which would penalize the code-
// splitting this budget is supposed to encourage.

import { readFileSync } from 'node:fs'
import { gzipSync } from 'node:zlib'
import { dirname, join } from 'node:path'

const DIST_DIR = new URL('../dist', import.meta.url).pathname
const JS_BUDGET_BYTES = 150 * 1024
const CRITICAL_PATH_BUDGET_BYTES = 250 * 1024

function gzipSize(path) {
  // Level 9 (max compression): matches what a production CDN/server
  // actually serves, and what `vite build`'s own reported gzip size uses
  // — level 6 (Node's zlib default) reports ~2-3% larger and would make
  // this check inconsistent with the number `npm run build` prints.
  return gzipSync(readFileSync(path), { level: 9 }).length
}

function eagerAssetPaths(html) {
  const paths = new Set()
  // module scripts and stylesheets index.html loads unconditionally —
  // modulepreload counts too (Vite emits it when it statically knows a
  // chunk is needed right away, as opposed to behind a runtime lazy()).
  const pattern = /<(?:script[^>]*\ssrc|link[^>]*\shref)="([^"]+)"[^>]*>/g
  for (const match of html.matchAll(pattern)) {
    const src = match[1]
    if (src.endsWith('.js') || src.endsWith('.css')) paths.add(src)
  }
  return [...paths]
}

const indexHtmlPath = join(DIST_DIR, 'index.html')
const html = readFileSync(indexHtmlPath, 'utf8')
const eagerPaths = eagerAssetPaths(html)

let jsBytes = 0
let cssBytes = 0
for (const relPath of eagerPaths) {
  const absPath = join(dirname(indexHtmlPath), relPath.replace(/^\//, ''))
  const size = gzipSize(absPath)
  if (relPath.endsWith('.js')) jsBytes += size
  else cssBytes += size
}
const criticalPathBytes = jsBytes + cssBytes

function report(label, bytes, budget) {
  const kb = (bytes / 1024).toFixed(2)
  const budgetKb = (budget / 1024).toFixed(0)
  const status = bytes <= budget ? 'OK' : 'OVER BUDGET'
  console.log(`${label}: ${kb} KB gzip (budget ${budgetKb} KB) — ${status}`)
  return bytes <= budget
}

console.log(`Eager assets (from index.html): ${eagerPaths.join(', ')}`)
const jsOk = report('Initial JS', jsBytes, JS_BUDGET_BYTES)
const criticalPathOk = report('Critical path (JS+CSS)', criticalPathBytes, CRITICAL_PATH_BUDGET_BYTES)

if (!jsOk || !criticalPathOk) {
  console.error('\nNFR-1.1 budget exceeded.')
  process.exit(1)
}
