import { readFileSync } from 'node:fs';
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

// The page's version, `build.json` (decision 0085, step 1): the release it is (VERSION) and its build (the app's script's
// content hash, as lib/update tells builds apart), so an app that carries its own pages can tell whether the server's
// are newer than its own, or the same.
const buildInfo = (): Plugin => ({
  name: 'cratarr-build-info',
  generateBundle(_, bundle) {
    const main = Object.values(bundle).find((c) => c.type === 'chunk' && c.isEntry && c.name === 'main');
    const build = main ? (/-([A-Za-z0-9_-]+)\.js$/.exec(main.fileName)?.[1] ?? main.fileName) : null;
    let version: string | null = null;
    try {
      version = readFileSync(new URL('../VERSION', import.meta.url), 'utf8').trim() || null;
    } catch {
      /* a checkout without it */
    }
    this.emitFile({ type: 'asset', fileName: 'build.json', source: JSON.stringify({ version, build }) });
  },
});

// `pnpm dev` proxies the API to a locally running Cratarr server, and its sign-in and sign-out pages (the server's own:
// an app pointed at the dev server signs in there, 2026-10-26).
const server = process.env.CRATARR_URL ?? 'http://127.0.0.1:8080';
export default defineConfig({
  plugins: [svelte(), precacheList(), buildInfo()],
  // Two pages: the app, and the guest page (0.9.19), apart from the app's shell, which asks for a sign-in.
  build: { rollupOptions: { input: { main: 'index.html', guest: 'guest.html' } } },
  server: {
    // The Host as the browser sent it: the server takes a form post only from its own address (its sign-in's same-origin
    // check), which a rewritten Host to its own port made another site.
    proxy: Object.fromEntries(['/api', '/signin', '/signout'].map((p) => [p, { target: server, changeOrigin: false }])),
  },
});
