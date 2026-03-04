import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      '@quarto/quarto-automerge-schema': path.resolve(
        __dirname,
        '../quarto-automerge-schema/src/index.ts',
      ),
      '@quarto/quarto-sync-client': path.resolve(
        __dirname,
        '../quarto-sync-client/src/index.ts',
      ),
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    testTimeout: 60_000,
    hookTimeout: 60_000,
    passWithNoTests: true,
  },
});
