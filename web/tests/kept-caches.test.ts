// What a browser kept before records (public/sw.js, `page 80`, `81`): the Library's list in the old shape, album views and
// covers under ids gone, streams under old track ids. The first read after the update must not meet any of it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (f: string) => readFileSync(new URL(f, import.meta.url), 'utf8');
const SW = read('../public/sw.js');
const OFFLINE = read('../src/lib/offline.svelte.ts');
const name = (src: string, key: string) => src.match(new RegExp(`^const ${key} = '([^']+)';`, 'm'))?.[1];

/** The names 0.15 kept under. */
const BEFORE = ['cratarr-v4', 'cratarr-covers', 'cratarr-data', 'cratarr-offline'];

/** What activation leaves of `keys`: the real worker's own code, on stand-ins. */
async function activate(keys: string[]) {
  const on: Record<string, (e: unknown) => void> = {};
  const left = new Set(keys);
  const self = { addEventListener: (type: string, f: (e: unknown) => void) => (on[type] = f), navigator: {}, clients: { claim: async () => {} }, skipWaiting() {} };
  const caches = { keys: async () => [...left], delete: async (k: string) => left.delete(k) };
  new Function('self', 'caches', SW)(self, caches);
  let done: Promise<unknown> = Promise.resolve();
  on.activate({ waitUntil: (p: Promise<unknown>) => (done = p) });
  await done;
  return [...left].sort();
}

test("activation lets go of everything kept before records, the Library's list first", async () => {
  const now = ['CACHE', 'COVERS', 'DATA', 'OFFLINE'].map((k) => name(SW, k)!);
  assert.deepEqual(await activate([...BEFORE, ...now]), [...now].sort());
});

test('none of the names before records is used again', () => {
  for (const k of ['CACHE', 'COVERS', 'DATA', 'OFFLINE']) assert.ok(!BEFORE.includes(name(SW, k)!), k);
});

test('the page and the worker name the same caches, or a kept track is put where nothing reads it', () => {
  for (const k of ['COVERS', 'DATA', 'OFFLINE']) assert.equal(name(OFFLINE, k), name(SW, k), k);
});

test('the list of tracks held goes with their cache, or it lists what the worker let go', () => {
  assert.notEqual(name(OFFLINE, 'INDEX'), name(OFFLINE, 'OLD_INDEX'));
  assert.match(OFFLINE, /localStorage\.removeItem\(OLD_INDEX\)/);
});

/** Whether the real worker answers a stream's address from what is kept, on stand-ins. */
function answeredFromKept(address: string) {
  const on: Record<string, (e: unknown) => void> = {};
  const self = { addEventListener: (type: string, f: (e: unknown) => void) => (on[type] = f), location: { origin: 'http://cratarr.test' }, navigator: {}, skipWaiting() {} };
  new Function('self', 'caches', SW)(self, { match: async () => new Response('') });
  let taken = false;
  on.fetch({ request: { method: 'GET', url: `http://cratarr.test${address}` }, respondWith: () => (taken = true), waitUntil() {} });
  return taken;
}

test("a stream that names its copy is that copy's, never what is kept under the track's address", () => {
  assert.equal(answeredFromKept('/api/stream/cratarr/tr-1?from_ms=0&from=in'), false);
});

test("a Play from queue's first ask, from the top of its track, goes to Cratarr too", () => {
  assert.equal(answeredFromKept('/api/stream/cratarr/tr-1?from=cratarr%2F%2Fin'), false);
});

test('the player asking for a track goes on being answered from what is kept', () => {
  assert.equal(answeredFromKept('/api/stream/cratarr/tr-1?from_ms=0'), true);
});
