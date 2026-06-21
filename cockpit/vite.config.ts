import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/mcp': 'http://127.0.0.1:2718',
      '/health': 'http://127.0.0.1:2718',
      '/api': 'http://127.0.0.1:2718',
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
  },
});
