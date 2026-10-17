import { execSync } from "node:child_process";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The bundle names the commit it was built from, so a deployed page and the
// hashes it shows can be tied to one kernel version.
function commit(): string {
  const fromEnv = process.env.COMMIT_SHA;
  if (fromEnv) return fromEnv.slice(0, 7);
  try {
    return execSync("git rev-parse --short HEAD", { stdio: ["ignore", "pipe", "ignore"] }).toString().trim();
  } catch {
    return "unknown";
  }
}

// `base: "./"` so the build works from any static path (GitHub Pages, a
// subfolder, file://). Workers are ES modules so they can share protocol.ts.
export default defineConfig({
  plugins: [react()],
  base: "./",
  define: { __COMMIT__: JSON.stringify(commit()) },
  server: { host: "127.0.0.1", port: 5173, strictPort: true },
  worker: { format: "es" },
});
