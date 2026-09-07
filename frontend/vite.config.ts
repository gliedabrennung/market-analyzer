import { defineConfig } from 'vite'
import solid from 'vite-plugin-solid'
import tailwindcss from '@tailwindcss/vite'
import { visualizer } from 'rollup-plugin-visualizer'

export default defineConfig({
  plugins: [
    solid(),
    tailwindcss(),
    // NFR-1.1 (frontend-tz.md §6.1): rollup-plugin-visualizer is the
    // ТЗ's own named tool for the bundle-size budget. Only does anything
    // on `vite build` (dist/stats.html); inert for `vite dev`.
    visualizer({ filename: 'dist/stats.html', gzipSize: true, brotliSize: true }),
  ],
  server: {
    port: 5173,
  },
})
