import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

/** Backend URL for the local `quarto trace view` server. */
const backend = process.env.VITE_TRACE_BACKEND || 'http://localhost:4180'

// https://vite.dev/config/
export default defineConfig({
  // Relative base so the bundle can be served from any mount point.
  base: './',
  plugins: [react()],
  build: {
    target: 'esnext',
    outDir: 'dist',
    emptyOutDir: true,
  },
  server: {
    port: 5174,
    proxy: {
      '/api': {
        target: backend,
        changeOrigin: true,
      },
    },
  },
})
