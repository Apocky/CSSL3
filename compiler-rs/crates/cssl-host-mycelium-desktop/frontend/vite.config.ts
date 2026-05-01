// § Vite config for Mycelium frontend
// § Tauri-friendly : fixed port + strict + no host-binding (security default)
// § per spec/grand-vision/23 § TECH-STACK : Vite 5 · React 19 · TS 5
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Tauri's `beforeDevCommand` waits on this URL to come up.
  server: {
    port: 5173,
    strictPort: true,
    host: false,
  },
  // Production output goes to `frontend/dist` per tauri.conf.json's
  // `frontendDist` field.
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: true,
  },
  // Vitest config — same file, dual-purpose.
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/__tests__/**/*.{test,spec}.{ts,tsx}"],
  },
});
