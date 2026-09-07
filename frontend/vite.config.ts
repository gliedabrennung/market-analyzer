import { defineConfig } from 'vite'
import solid from 'vite-plugin-solid'
import tailwindcss from '@tailwindcss/vite'
import { visualizer } from 'rollup-plugin-visualizer'

export default defineConfig({
  plugins: [
    solid(),
    tailwindcss(),

    visualizer({ filename: 'dist/stats.html', gzipSize: true, brotliSize: true }),
  ],
  server: {
    port: 5173,
  },
})
