import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  build: { outDir: "dist", chunkSizeWarningLimit: 1200 },
  server: {
    port: 5173,
    proxy: {
      "/ws": { target: "ws://127.0.0.1:8080", ws: true },
      "/api": "http://127.0.0.1:8080",
      "/media": "http://127.0.0.1:8080",
    },
  },
});
