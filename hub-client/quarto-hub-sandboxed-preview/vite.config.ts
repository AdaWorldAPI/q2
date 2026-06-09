import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { copyFileSync } from 'fs';
import { resolve } from 'path';

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    viteSingleFile(), // Bundle everything into a single HTML file
    {
      name: 'copy-to-parent',
      closeBundle() {
        // After build, copy dist/index.html to ../public/q2-raw.html
        // Files in public/ are served as-is without Vite transformation
        const srcPath = resolve(__dirname, 'dist/index.html');
        const destPath = resolve(__dirname, '../public/q2-raw.html');
        try {
          copyFileSync(srcPath, destPath);
          console.log('✓ Copied to ../public/q2-raw.html');
        } catch (err) {
          console.error('Failed to copy to q2-raw.html:', err);
        }
      },
    },
  ],
  build: {
    outDir: 'dist', // Build to local dist/ folder
    emptyOutDir: true,
    rollupOptions: {
      input: 'index.html',
    },
  },
});
