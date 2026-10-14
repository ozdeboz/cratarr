// Cratarr service worker: makes the app installable and lets the shell open without the server. Everything under
// /api/ is live data and always goes to the network, except the album lists (Incoming, Collection): they change
// slowly and are large, so the last copy is shown at once while a fresh one is fetched (stale-while-revalidate). The shell is network-first (a new build shows up at once);
// Vite's fingerprinted assets and the icons never change, so they are cache-first.
const CACHE = 'cratarr-v2';

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(['/', '/manifest.webmanifest'])));
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
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

self.addEventListener('fetch', (event) => {
  const req = event.request;
  const url = new URL(req.url);
  if (req.method !== 'GET' || url.origin !== self.location.origin) return;

  if (url.pathname === '/api/incoming' || url.pathname === '/api/collection') {
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
  if (url.pathname.startsWith('/api/')) return;

  if (req.mode === 'navigate') {
    // Hash routes all load "/", so one cached shell serves every page.
    event.respondWith(fetch(req).then((r) => put('/', r)).catch(() => caches.match('/')));
    return;
  }
  if (url.pathname.startsWith('/assets/') || url.pathname.startsWith('/icons/')) {
    event.respondWith(caches.match(req).then((hit) => hit || fetch(req).then((r) => put(req, r))));
  }
});
