import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// In dev we proxy /api and /ws to the locally-running daemon so the SPA
// behaves the same as it does in prod (where it's served from the daemon
// itself). The daemon must be started with ERREXD_DEV_MODE=true so its CORS
// policy permits direct fetches that bypass this proxy.
export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': { target: 'http://localhost:9090', changeOrigin: true },
      '/ws': {
        target: 'ws://localhost:9091',
        ws: true,
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/ws/, '')
      }
    }
  }
});
