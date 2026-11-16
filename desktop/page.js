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
//
// Out of reach has a bound (2026-11-08): Cratarr gets WAIT_MS to begin its answer for a file of its page, and a proxy's
// refusal in front of it (403, 502 to 504, as the page's lib/reach reads them) counts as no answer. A network that goes
// silent drops what is sent without a word, so without the bound the window stayed blank until the connection gave up:
// 2 min 14 s, measured on the ThinkPad with Cratarr's address dropped. Once out of reach, Cratarr counts so for AWAY_MS,
// or until it answers anything: meanwhile the page's files are the app's at once, without asking. The service worker's
// script is the one exception, Cratarr's or none: a quick failure, so the browser keeps the worker it has.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

/** Cratarr's own, never the page's: its API, Subsonic's, the sign-in, the guest page, the device's covers. */
const CRATARRS = ['/api', '/rest', '/signin', '/signout', '/guest', '/__device'];

/** How long Cratarr has to begin its answer for a file of its page (ms) before it counts as out of reach (above). */
const WAIT_MS = 3_000;
/** How long Cratarr counts as out of reach after that, unless it answers sooner (ms). */
const AWAY_MS = 60_000;
/** The service worker's script: Cratarr's or none, never the app's (above). */
const WORKER = 'sw.js';

/** A status that means Cratarr was not reached rather than that it answered: the proxy in front of it refusing from away
 *  (a bare 403), or Cratarr stopped (502 to 504). As the page's lib/reach has it. */
const notReached = (status) => status === 403 || status === 502 || status === 503 || status === 504;

/** Rejects once `signal` is aborted: the bound holds even for a way out that does not listen to it. */
const aborted = (signal) => new Promise((_, reject) => signal.addEventListener('abort', () => reject(signal.reason), { once: true }));

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
 *  Cratarr's page as its health check last told it (a build, or null). `waitMs` and `awayMs`: the bounds above (shorter
 *  in the tests). */
function ownPage(dir, told, { waitMs = WAIT_MS, awayMs = AWAY_MS } = {}) {
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

  /** Until when Cratarr counts as out of reach (ms; above). */
  let awayUntil = 0;
  /** Whether Cratarr counts as out of reach now. */
  const away = () => Date.now() < awayUntil;
  /** Cratarr answered: out of reach no more. */
  const reached = () => {
    awayUntil = 0;
  };

  /** Cratarr's answer to `req` if it begins within the wait and is Cratarr's own; else null, and Cratarr out of reach.
   *  The request is let go when the wait is over. */
  async function ask(req, cratarr) {
    const stop = new AbortController();
    const timer = setTimeout(() => stop.abort(new Error('Cratarr did not answer in time')), waitMs);
    try {
      const r = await Promise.race([cratarr(req, stop.signal), aborted(stop.signal)]);
      if (!notReached(r.status)) {
        reached();
        return r;
      }
      r.body?.cancel().catch(() => {});
    } catch {
      /* no answer in time, or none at all */
    } finally {
      clearTimeout(timer);
    }
    awayUntil = Date.now() + awayMs;
    return null;
  }

  /** The answer to `req`, made on Cratarr's address: from the app's page where it answers (above), else Cratarr's,
   *  through `cratarr(req, signal)` (`signal`: let the request go). */
  async function answer(req, cratarr) {
    const file = own && req.method === 'GET' ? fileFor(new URL(req.url).pathname) : null;
    if (!file)
      return cratarr(req).then((r) => {
        if (!notReached(r.status)) reached();
        return r;
      });
    if (file.startsWith('assets/') || serves()) return respond(file);
    const r = away() ? null : await ask(req, cratarr);
    // A redirect (to the sign-in page, without a session) is Cratarr's to give, and so is any answer to another file.
    if (r && (r.ok || (r.status >= 300 && r.status < 400) || file !== 'index.html')) return r;
    r?.body?.cancel().catch(() => {});
    if (file === WORKER) throw new Error('Cratarr is out of reach');
    return respond(file);
  }

  return { own, files, serves, fileFor, answer, away, reached };
}

module.exports = { CRATARRS, WAIT_MS, AWAY_MS, buildOf, compare, prefer, ownPage };
