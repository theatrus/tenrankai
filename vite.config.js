import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  root: 'src/frontend',
  plugins: [react()],
  
  build: {
    outDir: '../../static/dist',
    emptyOutDir: true,
    rollupOptions: {
      input: resolve(__dirname, 'src/frontend/app.ts'),
      output: {
        entryFileNames: 'tenrankai.js',
        format: 'iife',
        name: 'Tenrankai',
        // Bundle everything into a single file
        inlineDynamicImports: true,
        manualChunks: undefined
      }
    },
    // Target modern browsers for better performance
    target: 'es2020',
    // Enable source maps for debugging
    sourcemap: true,
    // Minify the output (using esbuild by default)
    minify: true
  },
  
  // Development server configuration
  server: {
    port: 5173,
    host: true,
    cors: true,
    proxy: {
      // Simple pass-through: proxy everything to Rust server
      // except Vite's own development files
      '/': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false,
        bypass(req, res, options) {
          const url = req.url || '';
          // Only bypass for Vite's own development files
          if (url.startsWith('/@vite') || 
              url.startsWith('/@fs') ||
              url.startsWith('/src/') ||
              url.startsWith('/node_modules/')) {
            return url; // Let Vite handle these
          }
          // Everything else goes to Rust server
          return null;
        }
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