import { defineConfig } from "vite";
import preact from "@preact/preset-vite";
export default defineConfig({
  plugins: [preact()],
  server: { port: 5217, strictPort: true, proxy: { "/ws": { target: "ws://127.0.0.1:8217", ws: true }, "/api": "http://127.0.0.1:8217", "/media": "http://127.0.0.1:8217" } },
});
