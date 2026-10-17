import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `base: "./"` so the build works from any static path (GitHub Pages, a
// subfolder, file://). Workers are ES modules so they can share protocol.ts.
export default defineConfig({
  plugins: [react()],
  base: "./",
  server: { host: "127.0.0.1", port: 5173, strictPort: true },
  worker: { format: "es" },
});
