import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Every file the build makes, listed in `precache.json` for the service worker to keep (public/sw.js): the whole app on
// the device after its first start, so it works without the server (the owner, 2026-10-24).
const precacheList = (): Plugin => ({
  name: 'cratarr-precache-list',
  generateBundle(_, bundle) {
    const files = Object.keys(bundle).filter((f) => !f.endsWith('.map')).map((f) => `/${f}`);
    this.emitFile({ type: 'asset', fileName: 'precache.json', source: JSON.stringify(files) });
  },
});

// `pnpm dev` proxies the API to a locally running Cratarr server.
export default defineConfig({
  plugins: [svelte(), precacheList()],
  server: {
    proxy: { '/api': process.env.CRATARR_URL ?? 'http://127.0.0.1:8080' },
  },
});
