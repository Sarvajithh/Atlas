import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite config follows Tauri's documented conventions: fixed dev port, no
// auto-open browser (Tauri opens its own window), and ignoring the
// src-tauri directory in the watcher so Rust rebuilds don't retrigger the
// frontend dev server.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "src/dist",
  },
  resolve: {
    alias: {
      "@": "/src",
    },
  },
});
