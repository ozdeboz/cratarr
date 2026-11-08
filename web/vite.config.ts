import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { buildId, type Built } from './buildId';

// Every file the build makes, listed in `precache.json` for the service worker to keep (public/sw.js): the whole app on
// the device after its first start, so it works without the server (the owner, 2026-10-24).
const precacheList = (): Plugin => ({
  name: 'cratarr-precache-list',
  generateBundle(_, bundle) {
    const files = Object.keys(bundle).filter((f) => !f.endsWith('.map')).map((f) => `/${f}`);
    this.emitFile({ type: 'asset', fileName: 'precache.json', source: JSON.stringify(files) });
  },
});

// The page's version, `build.json` (decision 0085, step 1): the release it is (VERSION) and its build, so an app that
// carries its own pages can tell whether the server's are newer than its own, or the same. The build is what the page is
// made of (./buildId: every file, the hashes in their names taken out, the public ones beside them), so two builds of
// one commit are one build wherever they were made.
const buildInfo = (): Plugin => {
  let publicDir = '';
  return {
    name: 'cratarr-build-info',
    configResolved(c) {
      publicDir = c.publicDir;
    },
    generateBundle(_, bundle) {
      // Line ends as git keeps them (a Windows checkout's CRLF is no other page).
      const lf = (s: string) => s.replace(/\r\n/g, '\n');
      const files: Built[] = Object.values(bundle)
        .filter((f) => !f.fileName.endsWith('.map') && f.fileName !== 'precache.json')
        .map((f) => ({ fileName: f.fileName, content: f.type === 'chunk' ? lf(f.code) : typeof f.source === 'string' ? lf(f.source) : f.source }));
      for (const f of publicFiles(publicDir)) files.push(f);
      const build = buildId(files);
      // Without it the build stops (0.11.0: the image's page stage lacked it, and build.json said `version: null`
      // unnoticed, so no app could tell the server's pages were newer).
      let version: string | null = null;
      try {
        version = readFileSync(new URL('../VERSION', import.meta.url), 'utf8').trim() || null;
      } catch {
        /* told below */
      }
      if (!version) this.error('VERSION (beside web/) is missing or empty: build.json must name the release');
      this.emitFile({ type: 'asset', fileName: 'build.json', source: JSON.stringify({ version, build }) });
    },
  };
};

/** The public files (copied as they are: the service worker, the icons), as the page's build counts them: text with its
 *  line ends as git keeps them. */
function publicFiles(dir: string): Built[] {
  if (!dir) return [];
  const text = /\.(js|mjs|json|html|css|svg|txt|webmanifest)$/;
  let names: string[] = [];
  try {
    names = readdirSync(dir, { recursive: true }) as string[];
  } catch {
    return [];
  }
  return names
    .map((n) => n.split('\\').join('/'))
    .filter((n) => statSync(join(dir, n)).isFile())
    .map((n) => {
      const bytes = readFileSync(join(dir, n));
      return { fileName: n, content: text.test(n) ? bytes.toString('utf8').replace(/\r\n/g, '\n') : new Uint8Array(bytes) };
    });
}

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
