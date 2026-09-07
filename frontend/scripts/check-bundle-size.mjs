#!/usr/bin/env node

import { readFileSync } from 'node:fs'
import { gzipSync } from 'node:zlib'
import { dirname, join } from 'node:path'

const DIST_DIR = new URL('../dist', import.meta.url).pathname
const JS_BUDGET_BYTES = 150 * 1024
const CRITICAL_PATH_BUDGET_BYTES = 250 * 1024

function gzipSize(path) {
  return gzipSync(readFileSync(path), { level: 9 }).length
}

function eagerAssetPaths(html) {
  const paths = new Set()

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
