import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The web UI lives in ui/; Tauri loads ui/dist (see app/tauri.conf.json).
export default defineConfig({
  root: "ui",
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
});
