// The service worker's copies, two accounts on one device (public/sw.js, the real one, on stand-ins; decision 0101 point
// 14): each account's reads are its own, none is answered from another's, and nobody has none.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const SW = readFileSync(new URL('../public/sw.js', import.meta.url), 'utf8');
const ORIGIN = 'http://cratarr.test';
const name = (key: string) => SW.match(new RegExp(`^const ${key} = '([^']+)';`, 'm'))![1];
const DATA = name('DATA');
const COVERS = name('COVERS');
const OFFLINE = name('OFFLINE');
const WHO = name('WHO');

/** Cache Storage, by name; an entry by its path and query, as a stand-in keeps them. */
function storage() {
  const all = new Map<string, Map<string, Response>>();
  const path = (k: string | { url: string }) => {
    const u = new URL(typeof k === 'string' ? k : k.url, ORIGIN);
    return u.pathname + u.search;
  };
  const cache = (n: string) => {
    const c = all.get(n) ?? new Map<string, Response>();
    all.set(n, c);
    return {
      match: async (k: string | { url: string }) => c.get(path(k))?.clone(),
      put: async (k: string | { url: string }, r: Response) => void c.set(path(k), r),
      delete: async (k: string | { url: string }) => c.delete(path(k)),
      keys: async () => [...c.keys()].map((u) => ({ url: ORIGIN + u })),
    };
  };
  return {
    all,
    caches: {
      open: async (n: string) => cache(n),
      keys: async () => [...all.keys()],
      delete: async (n: string) => all.delete(n),
      match: async (k: string | { url: string }, o?: { cacheName?: string }) => {
        if (o?.cacheName) return all.has(o.cacheName) ? cache(o.cacheName).match(k) : undefined;
        for (const n of all.keys()) {
          const hit = await cache(n).match(k);
          if (hit) return hit;
        }
        return undefined;
      },
    },
  };
}

/** The real worker started on stand-ins: a read or a navigation fetched, a message told, the rest as a browser does. */
function worker(store = storage()) {
  const on: Record<string, ((e: unknown) => void)[]> = {};
  const self = {
    addEventListener: (type: string, f: (e: unknown) => void) => void (on[type] ??= []).push(f),
    location: { origin: ORIGIN },
    navigator: { onLine: true },
    clients: { claim: async () => {}, matchAll: async () => [] },
    skipWaiting() {},
  };
  /** What the network answers a path with, read as it is asked. */
  const net = { answer: (path: string) => Promise.resolve(new Response(JSON.stringify({ path }))) as Promise<Response> };
  const fetchHere = (r: string | { url: string }) => net.answer(new URL(typeof r === 'string' ? r : r.url, ORIGIN).pathname);
  new Function('self', 'caches', 'fetch', SW)(self, store.caches, fetchHere);
  let waits: Promise<unknown>[] = [];
  const settled = async () => {
    while (waits.length) {
      const now = waits;
      waits = [];
      await Promise.allSettled(now);
    }
  };
  return {
    store,
    net,
    settled,
    tell: async (data: unknown) => {
      for (const f of on.message) f({ data, waitUntil: (p: Promise<unknown>) => void waits.push(p) });
      await settled();
    },
    /** Asks for `path` through the worker; undefined: left to the browser. */
    ask: async (path: string, more: Record<string, unknown> = {}) => {
      let answer: Promise<Response> | undefined;
      for (const f of on.fetch)
        f({
          request: { method: 'GET', url: ORIGIN + path, mode: 'cors', cache: 'default', ...more },
          respondWith: (r: Promise<Response>) => (answer = r),
          waitUntil: (p: Promise<unknown>) => void waits.push(p),
        });
      const r = await answer;
      await settled();
      return r;
    },
    activate: async () => {
      for (const f of on.activate) f({ waitUntil: (p: Promise<unknown>) => void waits.push(p) });
      await settled();
    },
  };
}
const signedIn = (name: string | null) => ({ type: 'account', name });
const has = (w: ReturnType<typeof worker>, cache: string, path: string) => !!w.store.all.get(cache)?.has(path);

test("a navigation to the sign-in page is nobody's from there on: the one before's copies answer no one's first reads", async () => {
  const w = worker();
  await w.tell(signedIn('ann'));
  await w.ask('/api/albums');
  assert.ok(has(w, DATA + '@ann', '/api/albums'));
  await w.ask('/signin?next=%2F', { mode: 'navigate' });
  w.net.answer = () => Promise.resolve(new Response('{"his":true}'));
  const r = await w.ask('/api/albums');
  assert.deepEqual(await r!.json(), { his: true }, "not her list, kept and shown at once as it was");
  assert.deepEqual(await (await w.store.all.get(WHO)!.get('/__who'))!.clone().text(), '-', 'kept, for a worker woken later');
});

test('a read of the sign-in page that is no navigation changes nothing', async () => {
  const w = worker();
  await w.tell(signedIn('ann'));
  await w.ask('/signin');
  await w.ask('/api/albums');
  assert.ok(has(w, DATA + '@ann', '/api/albums'));
});
