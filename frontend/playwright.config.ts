import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './e2e', workers: 1, use: { baseURL: 'http://voxy.test', headless: true },
  outputDir: '../test-results', reporter: 'list',
});
