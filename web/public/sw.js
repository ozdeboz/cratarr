// Cratarr service worker: makes the app installable and lets the whole app open without the server: every file the
// build made (its `precache.json`, vite.config.ts) is kept when it installs, and again after a new build, once the server
// answers (the owner, 2026-10-24: the entire app downloaded, so offline works; before, only what had been opened was). Everything under
// /api/ is live data and always goes to the network, except the Library's album list: it changes
// slowly and are large, so the last copy is shown at once while a fresh one is fetched (stale-while-revalidate). The shell is network-first (a new build shows up at once);
// Vite's fingerprinted assets and the icons never change, so they are cache-first. The covers of the albums downloaded
// in the app are kept apart, as the page lists them (lib/coversAway.svelte.ts), so they show away from home, where only
// downloads play (decision 0033, step 4).
const CACHE = 'cratarr-v4';
const COVERS = 'cratarr-covers';
// The last copy of what the pages read (the owner, 2026-10-26: with the server down, playlists and the rest showed
// nothing): kept per account (the page names it, `account`), each read tried on the network first; one that does not
// answer within KEPT_AFTER_MS, or fails, or meets the proxy's refusal, is answered with its last copy, marked
// `X-Cratarr-Kept` (when it was kept) so the page says it is offline. Live reads (the event stream, players, tasks,
// sign-in, sound) are never kept.
const DATA = 'cratarr-data';
const KEPT_AFTER_MS = 4000;
// Known to be out of reach (the owner, 2026-10-28: no 4 s wait each time): no network, the page saying so (lib/reach,
// the app's network report), or a read here that did not reach Cratarr. Then a read waits only this long for the
// network (Cratarr back: its answer, fresh) before its last copy; with no network at all, not at all. The first read
// that reaches Cratarr again ends it, and the pages are told.
const KEPT_AWAY_MS = 400;
let away = false;
function setAway(on) {
  if (away === on) return;
  away = on;
  if (!on) self.clients.matchAll().then((all) => all.forEach((c) => c.postMessage({ type: 'reached' })));
}
const keptAfter = () => (self.navigator.onLine === false ? 0 : away ? KEPT_AWAY_MS : KEPT_AFTER_MS);
const KEPT = /^\/api\/(?:(?:albums|playlists|crates|artists|home|library|discover|insights|history|favorites|excluded|me|requests|visuals)(?:\/|$)|covers\/stamps$|cover\/|track\/[^/]+\/[^/]+\/waveform$)/;
const NEVER = /^\/api\/(events|players|player|state|tasks|auth|health|status|time|stream|file|preview|catalogue|diagnostics|search)/;
const ALBUM_COVER = /^\/api\/albums\/[^/]+\/[^/]+\/cover$/;
// Tracks kept for offline play in the installed page (0.9.8, lib/offline): each one's stream, whole from its start, under
// the track's address alone. The player's own stream request for it is answered from there, whatever position or form it
// asks (the player cuts what lies before the position); the page's fetch of the whole copy (?whole=1) passes.
const OFFLINE = 'cratarr-offline';
const STREAM = /^\/api\/stream\/[^/]+\/[^/]+$/;

/** Keeps every file of the build that is not kept yet, and lets go of those of earlier builds. */
async function keepTheApp() {
  const r = await fetch('/precache.json', { cache: 'no-store' });
  if (!r.ok) return;
  const files = await r.json();
  if (!Array.isArray(files)) return;
  const c = await caches.open(CACHE);
  const want = new Set(files);
  for (const k of await c.keys()) {
    const path = new URL(k.url).pathname;
    if (path.startsWith('/assets/') && !want.has(path)) await c.delete(k);
  }
  for (const path of files) {
    if (await c.match(path)) continue;
    const got = await fetch(path);
    if (got.ok) await c.put(path, got);
  }
}
/** At most this often after the shell loaded: a new build's files are fetched once the server answers. */
const KEEP_EVERY_MS = 10 * 60 * 1000;
let keptAt = 0;
const keepNow = () => {
  if (Date.now() - keptAt < KEEP_EVERY_MS) return Promise.resolve();
  keptAt = Date.now();
  return keepTheApp().catch(() => (keptAt = 0));
};

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((c) => c.addAll(['/', '/manifest.webmanifest']))
      .then(() => keepNow()),
  );
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE && k !== COVERS && k !== DATA && k !== OFFLINE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

const put = (key, response) => {
  if (response.ok) {
    const copy = response.clone();
    caches.open(CACHE).then((c) => c.put(key, copy));
  }
  return response;
};

/** A read answered from its last copy, marked with when it was kept. */
async function keptCopy(req) {
  const hit = await caches.match(req, { cacheName: DATA });
  if (!hit) return null;
  const headers = new Headers(hit.headers);
  headers.set('X-Cratarr-Kept', hit.headers.get('X-Cratarr-Kept-At') ?? '1');
  return new Response(hit.body, { status: hit.status, statusText: hit.statusText, headers });
}
/** Keeps a read's answer, with when. */
async function keep(req, r) {
  const headers = new Headers(r.headers);
  headers.set('X-Cratarr-Kept-At', String(Date.now()));
  const copy = new Response(await r.clone().arrayBuffer(), { status: r.status, statusText: r.statusText, headers });
  await (await caches.open(DATA)).put(req, copy);
}
/** Network first; its last copy where the network does not answer in time (the fetch going on to keep a fresh one), fails, or
 *  meets the proxy's refusal (403 or a 5xx without Cratarr behind it). Not found or signed out stay the server's answer. */
function keptRead(event, req) {
  const net = fetch(req).then(
    (r) => {
      if (r.ok) event.waitUntil(keep(req, r).catch(() => {}));
      // Cratarr's own answer (the proxy's refusal or its empty 5xx are not).
      if (r.ok || r.status === 401 || r.status === 404 || (r.status < 500 && r.status !== 403)) setAway(false);
      else setAway(true);
      return r;
    },
    () => (setAway(true), null),
  );
  event.waitUntil(net);
  return new Promise((resolve) => {
    let done = false;
    const give = (r) => !done && ((done = true), resolve(r));
    const timer = setTimeout(async () => {
      const k = await keptCopy(req);
      if (k) {
        // Waited the whole while: not answering is out of reach too.
        if (!away && self.navigator.onLine !== false) setAway(true);
        give(k);
      }
    }, keptAfter());
    net.then(async (r) => {
      clearTimeout(timer);
      if (r && (r.ok || r.status === 401 || r.status === 404 || (r.status < 500 && r.status !== 403))) return give(r);
      const k = await keptCopy(req);
      give(k ?? r ?? Response.error());
    });
  });
}

// The page's word on reach (lib/reach): out of reach, or Cratarr answered it.
self.addEventListener('message', (event) => {
  if (event.data?.type === 'reach' && typeof event.data.away === 'boolean') setAway(event.data.away);
});

// The account whose copies are kept: another one signing in, the last one's go.
self.addEventListener('message', (event) => {
  if (event.data?.type !== 'account' || typeof event.data.name !== 'string') return;
  event.waitUntil(
    caches.open(DATA).then(async (c) => {
      const was = await c.match('/__account');
      const name = was ? await was.text() : null;
      if (name !== null && name !== event.data.name) {
        await caches.delete(DATA);
        c = await caches.open(DATA);
      }
      await c.put('/__account', new Response(event.data.name));
    }),
  );
});

// The covers to keep (their paths, without a version): those no longer listed go, the missing ones are fetched.
self.addEventListener('message', (event) => {
  if (event.data?.type !== 'keep-covers' || !Array.isArray(event.data.urls)) return;
  const want = new Set(event.data.urls.map((u) => new URL(u, self.location.origin).pathname));
  event.waitUntil(
    caches.open(COVERS).then(async (c) => {
      const have = new Set();
      for (const k of await c.keys()) {
        const path = new URL(k.url).pathname;
        if (want.has(path)) have.add(path);
        else await c.delete(k);
      }
      for (const path of want) {
        if (have.has(path)) continue;
        try {
          const r = await fetch(path);
          if (r.ok) await c.put(path, r);
          // Out of reach, or signed out: the next list tries again.
          else if (r.status === 403 || r.status === 401) return;
        } catch {
          return;
        }
      }
    }),
  );
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  const url = new URL(req.url);
  if (req.method !== 'GET' || url.origin !== self.location.origin) return;

  if (url.pathname === '/api/albums') {
    const fresh = fetch(req).then((r) => put(req, r));
    // The page asks with cache: 'no-cache' when it wants the latest, right after showing the kept copy.
    if (req.cache === 'no-cache') {
      event.respondWith(fresh);
      return;
    }
    event.waitUntil(fresh.catch(() => {}));
    event.respondWith(caches.match(req).then((hit) => hit || fresh));
    return;
  }
  if (STREAM.test(url.pathname) && url.searchParams.get('whole') !== '1') {
    event.respondWith(caches.match(url.pathname, { cacheName: OFFLINE }).then((hit) => hit || fetch(req)));
    return;
  }
  if (ALBUM_COVER.test(url.pathname)) {
    // A kept cover is kept fresh, and serves while Cratarr is out of reach (the proxy's 403, or no network).
    const kept = () => caches.match(url.pathname, { cacheName: COVERS });
    event.respondWith(
      fetch(req)
        .then(async (r) => {
          if (!r.ok) return (await kept()) || r;
          if (await kept()) {
            const copy = r.clone();
            caches.open(COVERS).then((c) => c.put(url.pathname, copy));
          }
          return r;
        })
        .catch(async () => (await kept()) || Response.error()),
    );
    return;
  }
  if (url.pathname.startsWith('/api/') && !NEVER.test(url.pathname) && KEPT.test(url.pathname)) {
    event.respondWith(keptRead(event, req));
    return;
  }
  if (url.pathname.startsWith('/api/')) return;
  // Cratarr's sign-in page (decision 0039) is the server's own, never the shell.
  if (url.pathname === '/signin' || url.pathname === '/signout') return;

  if (req.mode === 'navigate') {
    // Hash routes all load "/", so one cached shell serves every page. Away from home the server's address may answer
    // with an error page (the proxy's 403 outside the LAN) rather than not at all: the shell then too, so the app's
    // downloads still play (decision 0033, step 4). A redirect (to the sign-in page, without a session) is followed.
    event.respondWith(
      fetch(req)
        .then((r) => {
          if (r.type === 'opaqueredirect') return r;
          // The server answers: what a new build brought is kept too.
          if (r.ok) event.waitUntil(keepNow());
          if (r.ok) return url.pathname === '/' ? put('/', r) : r;
          return caches.match('/').then((hit) => hit || r);
        })
        .catch(() => caches.match('/')),
    );
    return;
  }
  if (url.pathname.startsWith('/assets/') || url.pathname.startsWith('/icons/')) {
    event.respondWith(caches.match(req).then((hit) => hit || fetch(req).then((r) => put(req, r))));
  }
});
