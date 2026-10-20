import { defineConfig } from "vite";
import preact from "@preact/preset-vite";
export default defineConfig({
  plugins: [preact()],
  cacheDir: "/tmp/fe3d/.vite",
  server: { port: 5192, strictPort: true, proxy: { "/ws": { target: "ws://127.0.0.1:8172", ws: true }, "/api": "http://127.0.0.1:8172", "/media": "http://127.0.0.1:8172", "/library": "http://127.0.0.1:8172" } },
});
