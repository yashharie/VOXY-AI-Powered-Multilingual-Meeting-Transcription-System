import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/vite-plugin-svelte";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit()],

  // Tauri dev server host
  server: {
    port: 1420,
    strictPort: true,
    host: "localhost",
    hmr: {
      protocol: "ws",
      host: "localhost",
      port: 1421,
    },
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
