// The service worker's copies, two accounts on one device (public/sw.js, the real one, on stand-ins; decision 0101 point
// 14): each account's reads are its own, none is answered from another's, and nobody has none.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { name, storage, worker } from './harness/serviceWorker.ts';

const DATA = name('DATA');
const COVERS = name('COVERS');
const OFFLINE = name('OFFLINE');
const WHO = name('WHO');

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

/** A network answer held back until `go`: an answer that comes after whatever the test does meanwhile. */
function later(w: ReturnType<typeof worker>) {
  let go: (r: Response) => void = () => {};
  const held = new Promise<Response>((done) => (go = done));
  w.net.answer = () => held;
  return go;
}

test("a read is the account's who asked: its answer, come after another signed in, is never kept as the other's", async () => {
  const w = worker();
  await w.tell(signedIn('ann'));
  const answer = later(w);
  const asked = w.ask('/api/playlists');
  await w.tell(signedIn('bo'));
  answer(new Response('{"hers":true}'));
  await asked;
  assert.equal(has(w, DATA + '@bo', '/api/playlists'), false);
  assert.ok(has(w, DATA + '@ann', '/api/playlists'), 'hers, for her return');
});

test("the album list the same, each way it is asked", async () => {
  const w = worker();
  await w.tell(signedIn('ann'));
  for (const cache of ['default', 'no-cache']) {
    const answer = later(w);
    const asked = w.ask('/api/albums', { cache });
    await w.tell(signedIn('bo'));
    answer(new Response('{"hers":true}'));
    await asked;
    assert.equal(has(w, DATA + '@bo', '/api/albums'), false, cache);
    await w.tell(signedIn('ann'));
  }
});

test('nor kept for nobody, signed out as the answer came', async () => {
  const w = worker();
  await w.tell(signedIn('ann'));
  const answer = later(w);
  const asked = w.ask('/api/playlists');
  await w.tell(signedIn(null));
  answer(new Response('{}'));
  await asked;
  assert.deepEqual([...w.store.all.keys()].filter((n) => n.startsWith(DATA) && n !== DATA + '@ann'), []);
});

test('a worker woken by a read, told whose it is while it reads the name kept, stays told', async () => {
  const store = storage();
  (await store.caches.open(WHO)).put('/__who', new Response('@ann'));
  let slow: () => void = () => {};
  let first = true;
  const open = store.caches.open;
  store.caches.open = async (n: string) => {
    const c = await open(n);
    if (n !== WHO || !first) return c;
    first = false;
    // The name as it stood when read, delivered late.
    const match = async (k: string) => {
      const read = await c.match(k);
      await new Promise<void>((done) => (slow = done));
      return read;
    };
    return { ...c, match };
  };
  const w = worker(store);
  const asked = w.ask('/api/playlists');
  await new Promise((r) => setTimeout(r));
  await w.tell(signedIn('bo'));
  slow();
  await asked;
  await w.ask('/api/crates');
  assert.ok(has(w, DATA + '@bo', '/api/crates'), 'the name told, not the one read a moment too late');
});

test("out of reach each account has its own last copies, and none of another's", async () => {
  const w = worker();
  await w.tell(signedIn('ann'));
  w.net.answer = () => Promise.resolve(new Response('{"hers":true}'));
  await w.ask('/api/playlists');
  w.net.answer = () => Promise.reject(new TypeError('no network'));
  assert.deepEqual(await (await w.ask('/api/playlists'))!.json(), { hers: true });
  await w.tell(signedIn('bo'));
  assert.equal((await w.ask('/api/playlists'))!.type, 'error', 'he has none');
  await w.tell(signedIn(null));
  assert.equal((await w.ask('/api/playlists'))!.type, 'error', 'nor has nobody');
  await w.tell(signedIn('ann'));
  assert.deepEqual(await (await w.ask('/api/playlists'))!.json(), { hers: true }, 'and hers again at her return');
});

test('nobody keeps nothing', async () => {
  const w = worker();
  await w.ask('/api/playlists');
  await w.ask('/api/albums');
  await w.tell({ type: 'keep-covers', urls: ['/api/albums/cratarr/al-1/cover'] });
  assert.deepEqual([...w.store.all.keys()].filter((n) => n !== WHO), []);
});

test("the covers kept are each account's, its list touching no other's", async () => {
  const w = worker();
  const cover = '/api/albums/cratarr/al-1/cover';
  await w.tell(signedIn('ann'));
  await w.tell({ type: 'keep-covers', urls: [cover] });
  await w.tell(signedIn('bo'));
  await w.tell({ type: 'keep-covers', urls: [] });
  assert.ok(has(w, COVERS + '@ann', cover), 'his list says nothing of hers');
  w.net.answer = () => Promise.reject(new TypeError('no network'));
  assert.equal((await w.ask(cover))!.type, 'error', 'he has no copy of hers');
  await w.tell(signedIn('ann'));
  assert.ok((await w.ask(cover))!.ok);
});

test("a cover's answer goes to the copy of the account who asked", async () => {
  const w = worker();
  const cover = '/api/albums/cratarr/al-1/cover';
  await w.tell(signedIn('ann'));
  await w.tell({ type: 'keep-covers', urls: [cover] });
  await w.tell(signedIn('bo'));
  await w.tell({ type: 'keep-covers', urls: [cover] });
  await w.tell(signedIn('ann'));
  const answer = later(w);
  const asked = w.ask(cover);
  await w.tell(signedIn('bo'));
  answer(new Response('hers, new'));
  await asked;
  await new Promise((r) => setTimeout(r));
  assert.notEqual(await (await w.store.caches.match(cover, { cacheName: COVERS + '@bo' }))!.text(), 'hers, new');
});

test('activation keeps every account\'s copies and whose they are, and lets go of those under names raised since', async () => {
  const w = worker();
  const now = [DATA + '@ann', DATA + '@', COVERS + '@ann', WHO, OFFLINE, name('CACHE')];
  const before = ['cratarr-data', 'cratarr-data@ann', 'cratarr-covers', 'cratarr-covers@ann', 'cratarr-offline', 'cratarr-v4'];
  for (const n of [...now, ...before]) await w.store.caches.open(n);
  await w.activate();
  assert.deepEqual([...w.store.all.keys()].sort(), [...now].sort());
});
