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

// `pnpm dev` proxies the API to a locally running Cratarr server, and its sign-in and sign-out pages (the server's own:
// an app pointed at the dev server signs in there, 2026-10-26).
const server = process.env.CRATARR_URL ?? 'http://127.0.0.1:8080';
export default defineConfig({
  plugins: [svelte(), precacheList()],
  // Two pages: the app, and the guest page (0.9.19), apart from the app's shell, which asks for a sign-in.
  build: { rollupOptions: { input: { main: 'index.html', guest: 'guest.html' } } },
  server: {
    // The Host as the browser sent it: the server takes a form post only from its own address (its sign-in's same-origin
    // check), which a rewritten Host to its own port made another site.
    proxy: Object.fromEntries(['/api', '/signin', '/signout'].map((p) => [p, { target: server, changeOrigin: false }])),
  },
});
