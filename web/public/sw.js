// Cratarr service worker: makes the app installable and lets the whole app open without the server: every file the
// build made (its `precache.json`, vite.config.ts) is kept when it installs, and again after a new build, once the server
// answers (the owner, 2026-10-24: the entire app downloaded, so offline works; before, only what had been opened was). Everything under
// /api/ is live data and always goes to the network, except the Library's album list: it changes
// slowly and are large, so the last copy is shown at once while a fresh one is fetched (stale-while-revalidate). The shell is network-first (a new build shows up at once);
// Vite's fingerprinted assets and the icons never change, so they are cache-first. The covers of the albums downloaded
// in the app are kept apart, as the page lists them (lib/coversAway.svelte.ts), so they show away from home, where only
// downloads play (decision 0033, step 4).
// Every cache here carries a number, raised when what is kept stops being true (0.16: a record's id replaced an album's and
// `copies` the one `library`, so the first paint would read the old shape, and a read Cratarr is slow to answer an album
// view under an id gone). Activation lets go of every other name, as devices start fresh (decision 0101 point 23).
// lib/offline.svelte.ts names three of them too.
const CACHE = 'cratarr-v5';
const COVERS = 'cratarr-covers-2';
// The last copy of what the pages read (the owner, 2026-10-26: with the server down, playlists and the rest showed
// nothing): kept per account (the page names it, `account`), each read tried on the network first; one that does not
// answer within KEPT_AFTER_MS, or fails, or meets the proxy's refusal, is answered with its last copy, marked
// `X-Cratarr-Kept` (when it was kept) so the page says it is offline. Live reads (the event stream, players, tasks,
// sign-in, sound) are never kept.
const DATA = 'cratarr-data-2';
// Whose copies are kept and shown (decision 0101): the data's, the Library's album list and the downloaded albums'
// covers are each account's apart (`cratarr-data-2@name`; the key as lib/account names it: `@name`, `@` with sign-in off),
// hidden while another is signed in and shown again at its next sign-in. Nobody signed in (`-`): none kept, none shown.
const WHO = 'cratarr-who';
const NOBODY = '-';
let who = null;
const whoNow = async () => {
  if (who !== null) return who;
  const kept = await caches.open(WHO).then((c) => c.match('/__who')).then((r) => (r ? r.text() : NOBODY), () => NOBODY);
  // Told while that was read, by a page that knows better: what it told stands.
  return (who ??= kept);
};
/** `w`'s copies from now on, kept for a worker woken later. */
const setWho = (w) => {
  who = w;
  return caches.open(WHO).then((c) => c.put('/__who', new Response(w)));
};
/** The account's own cache of `base` (DATA, COVERS); null for nobody. */
const ours = async (base) => {
  const w = await whoNow();
  return w === NOBODY ? null : base + w;
};
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
const KEPT = /^\/api\/(?:(?:albums|playlists|crates|artists|home|library|discover|insights|history|favorites|excluded|me|requests|visuals|capabilities)(?:\/|$)|covers\/stamps$|cover\/|track\/[^/]+\/[^/]+\/waveform$)/;
const NEVER = /^\/api\/(events|players|player|state|tasks|auth|health|status|time|stream|file|preview|catalogue|diagnostics|search)/;
const ALBUM_COVER = /^\/api\/albums\/[^/]+\/[^/]+\/cover$/;
// Tracks kept for offline play in the installed page (0.9.8, lib/offline): each one's stream, whole from its start, under
// the track's address alone. The player's own stream request for it is answered from there, whatever position or form it
// asks (the player cuts what lies before the position); the page's fetch of the whole copy (?whole=1) passes.
const OFFLINE = 'cratarr-offline-2';
const STREAM = /^\/api\/stream\/[^/]+\/[^/]+$/;
// Every record's small cover, kept with the index (decision 0085: small ones for the index, full ones for what is downloaded), so a
// list away from Cratarr shows them. They answer under the full cover's address: a page cannot tell, and remembers no failure. In a
// browser they lie in the account's index, a row each by the record's number (lib/indexStore); the app holds them itself, and
// answers them at INDEX_COVER (android DeviceIndex).
const RECORD_COVER = /^\/api\/albums\/cratarr\/al-(\d+)\/cover$/;
const INDEX_DB = 'cratarr-index';
const INDEX_COVERS = 'covers';
const INDEX_COVER = '/__device/index/cover/';

/** The account's index as it stands, null where this browser has none: only looked for, as it is the page's to make. */
async function keptIndex(name) {
  if (!(await indexedDB.databases()).some((d) => d.name === name)) return null;
  return new Promise((done) => {
    const r = indexedDB.open(name);
    r.onsuccess = () => done(r.result);
    r.onerror = () => done(null);
  });
}

/** A record's small cover by its full cover's address, the account's; null where none is held. No index here: the app's own, or none. */
async function smallCover(path) {
  const record = RECORD_COVER.exec(path)?.[1];
  const w = await whoNow();
  if (!record || w === NOBODY) return null;
  const db = await keptIndex(INDEX_DB + w).catch(() => null);
  if (!db) {
    // A server that does not know the address answers its page.
    const r = await fetch(INDEX_COVER + record).catch(() => null);
    return r?.ok && r.headers.get('content-type')?.startsWith('image/') ? r : null;
  }
  const jpeg = await new Promise((done) => {
    try {
      const r = db.transaction(INDEX_COVERS).objectStore(INDEX_COVERS).get(Number(record));
      r.onsuccess = () => done(r.result);
      r.onerror = () => done(null);
    } catch {
      // Made before covers had a store, and the page has not been back since.
      done(null);
    }
  });
  db.close();
  return jpeg ? new Response(jpeg, { headers: { 'Content-Type': 'image/jpeg' } }) : null;
}

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
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE && k !== WHO && k !== OFFLINE && !k.startsWith(COVERS) && !k.startsWith(DATA)).map((k) => caches.delete(k))))
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

/** A read answered from its last copy (in `mine`, the cache of the account who asked), marked with when it was kept. */
async function keptCopy(req, mine) {
  const cacheName = await mine;
  const hit = cacheName && (await caches.match(req, { cacheName }));
  if (!hit) return null;
  const headers = new Headers(hit.headers);
  headers.set('X-Cratarr-Kept', hit.headers.get('X-Cratarr-Kept-At') ?? '1');
  return new Response(hit.body, { status: hit.status, statusText: hit.statusText, headers });
}
/** Keeps a read's answer, with when, as the account's who asked (nobody's: not kept). */
async function keep(req, r, mine) {
  const cacheName = await mine;
  if (!cacheName) return;
  const headers = new Headers(r.headers);
  headers.set('X-Cratarr-Kept-At', String(Date.now()));
  const copy = new Response(await r.clone().arrayBuffer(), { status: r.status, statusText: r.statusText, headers });
  await (await caches.open(cacheName)).put(req, copy);
}
/** Network first; its last copy where the network does not answer in time (the fetch going on to keep a fresh one), fails, or
 *  meets the proxy's refusal (403 or a 5xx without Cratarr behind it). Not found or signed out stay the server's answer. */
function keptRead(event, req) {
  // Whose it is goes by who asked: the answer is that session's, whoever has signed in by the time it comes.
  const mine = ours(DATA);
  const net = fetch(req).then(
    (r) => {
      if (r.ok) event.waitUntil(keep(req, r, mine).catch(() => {}));
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
      const k = await keptCopy(req, mine);
      if (k) {
        // Waited the whole while: not answering is out of reach too.
        if (!away && self.navigator.onLine !== false) setAway(true);
        give(k);
      }
    }, keptAfter());
    net.then(async (r) => {
      clearTimeout(timer);
      if (r && (r.ok || r.status === 401 || r.status === 404 || (r.status < 500 && r.status !== 403))) return give(r);
      const k = await keptCopy(req, mine);
      give(k ?? r ?? Response.error());
    });
  });
}

// The page's word on reach (lib/reach): out of reach, or Cratarr answered it.
self.addEventListener('message', (event) => {
  if (event.data?.type === 'reach' && typeof event.data.away === 'boolean') setAway(event.data.away);
});

// The account signed in (its name, '' with sign-in off; null for nobody): its copies from now on, the others' kept.
self.addEventListener('message', (event) => {
  if (event.data?.type !== 'account') return;
  const name = event.data.name;
  event.waitUntil(setWho(typeof name === 'string' ? `@${name}` : NOBODY));
});

// The covers to keep, the account's (their paths, without a version): those no longer listed go, the missing ones are
// fetched.
self.addEventListener('message', (event) => {
  if (event.data?.type !== 'keep-covers' || !Array.isArray(event.data.urls)) return;
  const want = new Set(event.data.urls.map((u) => new URL(u, self.location.origin).pathname));
  event.waitUntil(
    ours(COVERS).then(async (name) => {
      if (!name) return;
      const c = await caches.open(name);
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
    // The own copy of the account who asked, as any read's (nobody's: none).
    const mine = ours(DATA);
    const fresh = fetch(req).then(async (r) => {
      const name = await mine;
      if (name && r.ok) {
        const copy = r.clone();
        event.waitUntil(caches.open(name).then((c) => c.put(req, copy)));
      }
      return r;
    });
    // The page asks with cache: 'no-cache' when it wants the latest, right after showing the kept copy.
    if (req.cache === 'no-cache') {
      event.respondWith(fresh);
      return;
    }
    event.waitUntil(fresh.catch(() => {}));
    event.respondWith(
      mine.then(async (name) => {
        const hit = name && (await caches.match(req, { cacheName: name }));
        if (!hit) return fresh;
        // Shown at once, whatever Cratarr would answer: a page must not take this for Cratarr answering (lib/api).
        const headers = new Headers(hit.headers);
        headers.set('X-Cratarr-Copy', '1');
        return new Response(hit.body, { status: hit.status, statusText: hit.statusText, headers });
      }),
    );
    return;
  }
  // A stream naming its copy (an intake card's snippet, a Play from queue) is that copy's: what is kept under the track's
  // address may be another's bytes, so it goes to Cratarr.
  if (STREAM.test(url.pathname) && url.searchParams.get('whole') !== '1' && !url.searchParams.has('from')) {
    event.respondWith(caches.match(url.pathname, { cacheName: OFFLINE }).then((hit) => hit || fetch(req)));
    return;
  }
  if (ALBUM_COVER.test(url.pathname)) {
    // A kept cover (the account's) is kept fresh, and serves while Cratarr is out of reach (the proxy's 403, or no
    // network); then the small one. Not found is Cratarr's own word: it has none.
    const mine = ours(COVERS);
    const kept = async () => {
      const cacheName = await mine;
      return cacheName ? caches.match(url.pathname, { cacheName }) : undefined;
    };
    event.respondWith(
      fetch(req)
        .then(async (r) => {
          if (!r.ok) return (await kept()) || (r.status !== 404 && (await smallCover(url.pathname))) || r;
          // Kept whole for the player away from home: a list's size of it never takes its place.
          if (!url.searchParams.has('size') && (await kept())) {
            const copy = r.clone();
            mine.then((name) => name && caches.open(name).then((c) => c.put(url.pathname, copy)));
          }
          return r;
        })
        .catch(async () => (await kept()) || (await smallCover(url.pathname)) || Response.error()),
    );
    return;
  }
  if (url.pathname.startsWith('/api/') && !NEVER.test(url.pathname) && KEPT.test(url.pathname)) {
    event.respondWith(keptRead(event, req));
    return;
  }
  if (url.pathname.startsWith('/api/')) return;
  // Cratarr's sign-in page (decision 0039) is the server's own, never the shell. Whoever comes through it is not known
  // yet, and the one before may be someone whose session ended with no page open: nobody's, until the page says.
  if (url.pathname === '/signin' && req.mode === 'navigate') event.waitUntil(setWho(NOBODY));
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
