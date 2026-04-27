import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';
import path from 'node:path';

// jsdom env so DOM APIs are available; the tests still mostly run pure
// logic against module singletons (stores, api client, normalizers). Svelte
// is plugged in for component tests when we need them. SvelteKit's $app
// modules are aliased to local stubs because they otherwise pull the full
// app runtime into the test bundle.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    // 'browser' must come before 'node' so Svelte 5 resolves to its client
    // runtime (index-client.js) rather than the server runtime inside jsdom.
    // Without this, mount() throws "not available on the server".
    conditions: ['browser'],
    alias: {
      $lib: path.resolve(__dirname, 'src/lib'),
      $app: path.resolve(__dirname, 'src/test/app-stubs')
    }
  },
  test: {
    environment: 'jsdom',
    globals: false,
    include: ['src/**/*.test.ts'],
    setupFiles: ['./src/test/setup.ts']
  }
});
