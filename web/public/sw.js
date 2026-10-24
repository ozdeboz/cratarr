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
const ALBUM_COVER = /^\/api\/albums\/[^/]+\/[^/]+\/cover$/;

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
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE && k !== COVERS).map((k) => caches.delete(k))))
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
