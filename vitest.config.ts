import { defineConfig } from 'vitest/config';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      '@': resolve(__dirname, 'frontend/react'),
      '@components': resolve(__dirname, 'frontend/react/components'),
      '@hooks': resolve(__dirname, 'frontend/react/hooks'),
      '@api': resolve(__dirname, 'frontend/react/api'),
      '@types': resolve(__dirname, 'frontend/react/types'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['frontend/react/__tests__/setup.ts'],
    include: ['frontend/react/__tests__/**/*.test.{ts,tsx}'],
  },
});
