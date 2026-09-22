import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  plugins: [svelte()],
  build: {
    rollupOptions: { input: { main: "index.html", hud: "hud.html" } },
    outDir: "dist",
    emptyOutDir: true
  }
});
