import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Component tests for views/panels using fixture IPC responses (§30).
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
  },
  resolve: {
    alias: {
      "@": "/src",
    },
  },
});
