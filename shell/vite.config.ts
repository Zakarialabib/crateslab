import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

// The Tauri shell owns the data: manifests and the mock fleet are read in Rust
// and reached over IPC. Vite only builds the webview assets.
export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2021" },
});
