// Cratarr service worker: makes the app installable and lets the shell open without the server. Everything under
// /api/ is live data and always goes to the network. The shell is network-first (a new build shows up at once);
// Vite's fingerprinted assets and the icons never change, so they are cache-first.
const CACHE = 'cratarr-v1';

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
  if (req.method !== 'GET' || url.origin !== self.location.origin || url.pathname.startsWith('/api/')) return;

  if (req.mode === 'navigate') {
    // Hash routes all load "/", so one cached shell serves every page.
    event.respondWith(fetch(req).then((r) => put('/', r)).catch(() => caches.match('/')));
    return;
  }
  if (url.pathname.startsWith('/assets/') || url.pathname.startsWith('/icons/')) {
    event.respondWith(caches.match(req).then((hit) => hit || fetch(req).then((r) => put(req, r))));
  }
});
