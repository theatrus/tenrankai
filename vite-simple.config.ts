import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

export default defineConfig({
  root: 'src/frontend',
  plugins: [react()],
  
  build: {
    outDir: '../../static/dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        'image-detail': resolve(__dirname, 'src/frontend/pages/image-detail.tsx'),
        'gallery': resolve(__dirname, 'src/frontend/pages/gallery.tsx')
      },
      output: {
        entryFileNames: '[name].js',
        chunkFileNames: '[name]-[hash].js',
        assetFileNames: '[name]-[hash].[ext]'
      }
    },
    target: 'es2020',
    sourcemap: true
  },
  
  // Simplified development server - only proxy what we need
  server: {
    port: 5173,
    host: true,
    proxy: {
      // Only proxy specific routes, not everything
      '^/gallery.*': 'http://localhost:3000',
      '^/api.*': 'http://localhost:3000',
      '^/_login.*': 'http://localhost:3000',
      '^/static(?!/dist).*': 'http://localhost:3000',
    }
  },
  
  define: {
    'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV || 'development')
  },
  
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src/frontend'),
      '@components': resolve(__dirname, 'src/frontend/components'),
      '@hooks': resolve(__dirname, 'src/frontend/hooks'),
      '@api': resolve(__dirname, 'src/frontend/api'),
      '@types': resolve(__dirname, 'src/frontend/types')
    }
  }
});