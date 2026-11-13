import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';
export default defineConfig({
  plugins: [svelte()],
  resolve: { alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) }, conditions: ['browser'] },
  server: { port: 1420, strictPort: true },
  build: { outDir: '../dist', emptyOutDir: true },
  test: { include: ['src/**/*.test.ts'], environment: 'node' },
});