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
    // Target modern browsers for better performance
    target: 'es2020',
    // Enable source maps for debugging
    sourcemap: true
  },
  
  // Development server configuration
  server: {
    port: 5173,
    host: true, // Allow external connections
    cors: true,
    proxy: {
      // Proxy API calls to Rust server during development
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false
      },
      '/gallery': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false
      },
      '/_login': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false
      },
      // Proxy static assets that aren't part of Vite build
      '/static': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false,
        // Exclude Vite's dist files from proxying
        bypass(req, res, options) {
          const url = req.url || '';
          if (url.startsWith('/static/dist/')) {
            return url;
          }
        }
      },
      // Proxy all other routes to Rust server for SSR
      '^(?!/src|/@|/node_modules).*': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false
      }
    }
  },
  
  // Define environment variables for React components
  define: {
    'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV || 'development')
  },
  
  // Resolve configuration
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src/frontend'),
      '@components': resolve(__dirname, 'src/frontend/components'),
      '@hooks': resolve(__dirname, 'src/frontend/hooks'),
      '@api': resolve(__dirname, 'src/frontend/api'),
      '@types': resolve(__dirname, 'src/frontend/types')
    }
  },
  
  // CSS configuration
  css: {
    modules: {
      localsConvention: 'camelCase'
    }
  }
});