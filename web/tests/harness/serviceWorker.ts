// The real service worker (public/sw.js) on stand-ins: its caches, its network and a browser's events. A test says what the network
// answers, tells it what a page would, and asks what it answers.
import { readFileSync } from 'node:fs';

const SW = readFileSync(new URL('../../public/sw.js', import.meta.url), 'utf8');
export const ORIGIN = 'http://cratarr.test';
/** One of the names at the top of the worker. */
export const name = (key: string) => SW.match(new RegExp(`^const ${key} = '([^']+)';`, 'm'))![1];

/** Cache Storage, by name; an entry by its path and query, as a stand-in keeps them. */
export function storage() {
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
export function worker(store = storage()) {
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
  /** All of `waits`, those added while waiting too. */
  const settled = async (waits: Promise<unknown>[]) => {
    for (let done = 0; done < waits.length; done = waits.length) await Promise.allSettled(waits);
  };
  return {
    store,
    net,
    tell: async (data: unknown) => {
      const waits: Promise<unknown>[] = [];
      for (const f of on.message) f({ data, waitUntil: (p: Promise<unknown>) => void waits.push(p) });
      await settled(waits);
    },
    /** Asks for `path` through the worker; undefined: left to the browser. */
    ask: async (path: string, more: Record<string, unknown> = {}) => {
      let answer: Promise<Response> | undefined;
      const waits: Promise<unknown>[] = [];
      for (const f of on.fetch)
        f({
          request: { method: 'GET', url: ORIGIN + path, mode: 'cors', cache: 'default', ...more },
          respondWith: (r: Promise<Response>) => (answer = r),
          waitUntil: (p: Promise<unknown>) => void waits.push(p),
        });
      const r = await answer;
      await settled(waits);
      return r;
    },
    activate: async () => {
      const waits: Promise<unknown>[] = [];
      for (const f of on.activate) f({ waitUntil: (p: Promise<unknown>) => void waits.push(p) });
      await settled(waits);
    },
  };
}
