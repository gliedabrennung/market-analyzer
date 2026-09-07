#!/usr/bin/env node
// NFR-1.1 (frontend-tz.md §6.1): "Начальный JS-чанк < 150 КБ gzip; общий
// вес критического пути < 250 КБ" — "проверка в CI". Run after `vite
// build`; gzips each dist/ asset itself (matches what a browser actually
// transfers, independent of rollup-plugin-visualizer's own per-module
// accounting, which double-counts modules that end up merged into one
// gzip stream).

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { gzipSync } from 'node:zlib'
import { join } from 'node:path'

const DIST_DIR = new URL('../dist', import.meta.url).pathname
const JS_BUDGET_BYTES = 150 * 1024
const CRITICAL_PATH_BUDGET_BYTES = 250 * 1024

function collectAssets(dir, extensions) {
  const out = []
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      out.push(...collectAssets(full, extensions))
    } else if (extensions.some((ext) => entry.endsWith(ext))) {
      out.push(full)
    }
  }
  return out
}

function gzipSize(path) {
  // Level 9 (max compression): matches what a production CDN/server
  // actually serves, and what `vite build`'s own reported gzip size uses
  // — level 6 (Node's zlib default) reports ~2-3% larger and would make
  // this check inconsistent with the number `npm run build` prints.
  return gzipSync(readFileSync(path), { level: 9 }).length
}

const jsAssets = collectAssets(DIST_DIR, ['.js'])
const cssAssets = collectAssets(DIST_DIR, ['.css'])

const jsBytes = jsAssets.reduce((sum, f) => sum + gzipSize(f), 0)
const cssBytes = cssAssets.reduce((sum, f) => sum + gzipSize(f), 0)
const criticalPathBytes = jsBytes + cssBytes

function report(label, bytes, budget) {
  const kb = (bytes / 1024).toFixed(2)
  const budgetKb = (budget / 1024).toFixed(0)
  const status = bytes <= budget ? 'OK' : 'OVER BUDGET'
  console.log(`${label}: ${kb} KB gzip (budget ${budgetKb} KB) — ${status}`)
  return bytes <= budget
}

const jsOk = report('JS', jsBytes, JS_BUDGET_BYTES)
const criticalPathOk = report('Critical path (JS+CSS)', criticalPathBytes, CRITICAL_PATH_BUDGET_BYTES)

if (!jsOk || !criticalPathOk) {
  console.error('\nNFR-1.1 budget exceeded.')
  process.exit(1)
}
