import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The bundle is loaded from the application's own origin inside the window, so
// every asset must be relative and nothing may be fetched from a remote host.
export default defineConfig({
  plugins: [react()],
  base: './',
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    target: 'chrome110',
    outDir: 'dist',
    emptyOutDir: true,
    assetsInlineLimit: 0,
    sourcemap: false,
  },
});
