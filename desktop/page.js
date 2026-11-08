// The app's own copy of Cratarr's page (decision 0085, step 1, as the Android app's BundledPage.kt): the web app's build,
// packed in the app (copy-page.js; resources/page), answered in place of the server's page files on Cratarr's own
// address. Cookies, the session, every address and the secure context stay as they are, and the page opens with
// Cratarr out of reach. The API, the streams, the sign-in and the guest page still go to Cratarr.
//
// Which page goes by the two builds (each one's `build.json`: the release and its build): Cratarr's release newer, its
// page (this app is the older); older, the app's; the same release and build, the app's; the same release built again,
// Cratarr's, as the fresher; Cratarr not heard from yet, the app's. Two things beyond Android's:
// - a file of Vite's (`assets/`, named by what it holds) is always the app's where the app has it, the same bytes as
//   Cratarr's, so a page loaded before the choice changed still finds its pieces;
// - Cratarr's page chosen but out of reach (no answer, or an error page in its place), the app's page opens instead: a
//   blank window never.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

/** Cratarr's own, never the page's: its API, Subsonic's, the sign-in, the guest page, the device's covers. */
const CRATARRS = ['/api', '/rest', '/signin', '/signout', '/guest', '/__device'];

/** A page's build ({version, build}: its release X.Y.Z and an id made from its files, the same for two builds of one
 *  commit, compared as it is), from a `build.json` or the health check's `web`; null when it says no release or no
 *  build. */
function buildOf(o) {
  if (!o || typeof o !== 'object') return null;
  const { version, build } = o;
  return typeof version === 'string' && version && typeof build === 'string' && build ? { version, build } : null;
}

/** Two releases (X.Y.Z) in order: negative when `a` is the older. A part that is not a number counts as 0. */
function compare(a, b) {
  const parts = (v) => v.split('.').map((n) => (/^\d+$/.test(n) ? Number(n) : 0));
  const x = parts(a);
  const y = parts(b);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d) return Math.sign(d);
  }
  return 0;
}

/** The app's page over Cratarr's `theirs` (above). */
function prefer(mine, theirs) {
  const c = compare(theirs.version, mine.version);
  return c < 0 || (c === 0 && theirs.build === mine.build);
}

const TYPES = {
  html: 'text/html; charset=utf-8',
  js: 'text/javascript; charset=utf-8',
  mjs: 'text/javascript; charset=utf-8',
  css: 'text/css; charset=utf-8',
  json: 'application/json; charset=utf-8',
  webmanifest: 'application/manifest+json; charset=utf-8',
  svg: 'image/svg+xml',
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  webp: 'image/webp',
  avif: 'image/avif',
  ico: 'image/x-icon',
  woff2: 'font/woff2',
  woff: 'font/woff',
  wasm: 'application/wasm',
  txt: 'text/plain; charset=utf-8',
};

/** The page in `dir` (none there: the app has no page of its own, and Cratarr's always answers). `told()` gives
 *  Cratarr's page as its health check last told it (a build, or null). */
function ownPage(dir, told) {
  const files = new Set();
  (function walk(rel) {
    let entries;
    try {
      entries = fs.readdirSync(path.join(dir, rel), { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      const p = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) walk(p);
      else if (e.isFile()) files.add(p);
    }
  })('');
  let own = null;
  try {
    if (files.has('build.json')) own = buildOf(JSON.parse(fs.readFileSync(path.join(dir, 'build.json'), 'utf8')));
  } catch {
    /* a page without its build is no page of the app's */
  }

  /** Whether the app's page is the one served (above). */
  function serves() {
    if (!own) return false;
    const theirs = buildOf(told());
    return !theirs || prefer(own, theirs);
  }

  /** The page's file for a GET of `pathname` on Cratarr's address, as the server answers it: a file of the page as it
   *  is, any other address without a dot the page itself (its own routes); null: Cratarr's alone. */
  function fileFor(pathname) {
    if (CRATARRS.some((c) => pathname === c || pathname.startsWith(`${c}/`))) return null;
    const rel = pathname.replace(/^\/+/, '');
    if (!rel) return 'index.html';
    if (files.has(rel)) return rel;
    return rel.includes('.') ? null : 'index.html';
  }

  /** One of the page's files, as the server sends it: Vite's fingerprinted files forever, the rest asked again each
   *  time. */
  async function respond(file) {
    const body = await fs.promises.readFile(path.join(dir, file));
    const type = TYPES[file.slice(file.lastIndexOf('.') + 1).toLowerCase()] ?? 'application/octet-stream';
    const cache = file.startsWith('assets/') ? 'public, max-age=31536000, immutable' : 'no-cache';
    return new Response(body, { status: 200, headers: { 'content-type': type, 'cache-control': cache } });
  }

  /** The answer to `req`, made on Cratarr's address: from the app's page where it answers (above), else Cratarr's,
   *  through `cratarr(req)`. */
  async function answer(req, cratarr) {
    const file = own && req.method === 'GET' ? fileFor(new URL(req.url).pathname) : null;
    if (!file) return cratarr(req);
    if (file.startsWith('assets/') || serves()) return respond(file);
    if (file !== 'index.html') return cratarr(req);
    try {
      const r = await cratarr(req);
      // A redirect (to the sign-in page, without a session) is Cratarr's to give.
      if (r.ok || (r.status >= 300 && r.status < 400)) return r;
      r.body?.cancel().catch(() => {});
    } catch {
      /* out of reach */
    }
    return respond(file);
  }

  return { own, files, serves, fileFor, answer };
}

module.exports = { CRATARRS, buildOf, compare, prefer, ownPage };
