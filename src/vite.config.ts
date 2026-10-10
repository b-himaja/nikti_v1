/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri v2 dev server convention: fixed port, strict, and don't let Vite
// clear a Tauri-relevant error overlay. See tauri.conf.json's
// build.devUrl/frontendDist, which point at this project.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist",
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./test/setup.ts"],
    globals: true,
  },
});
