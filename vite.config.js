import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  root: 'frontend/react',
  plugins: [react()],

  build: {
    outDir: '../../static/dist',
    emptyOutDir: true,
    // We intentionally bundle into a single file, so increase the warning limit
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      input: resolve(__dirname, 'frontend/react/app.ts'),
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
              url.startsWith('/frontend/') ||
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
      '@': resolve(__dirname, 'frontend/react'),
      '@components': resolve(__dirname, 'frontend/react/components'),
      '@hooks': resolve(__dirname, 'frontend/react/hooks'),
      '@api': resolve(__dirname, 'frontend/react/api'),
      '@types': resolve(__dirname, 'frontend/react/types')
    }
  },

  // CSS configuration
  css: {
    modules: {
      localsConvention: 'camelCase'
    }
  }
});
