// A record's small cover with Cratarr away (public/sw.js, the real one, over lib/indexStore, the real one, on stand-ins): a list shows
// it under the full cover's address, each account its own, and what the app holds comes from the app.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { databases } from '../harness/fakeIdb.ts';
import { name, worker } from '../harness/serviceWorker.ts';

const { putCovers } = await import('../../src/lib/indexStore.ts');

const COVER = '/api/albums/cratarr/al-10/cover';
const COVERS = name('COVERS');
const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xd9]);

// ann and the one user with sign-in off hold record 10's; dee has an index, and holds another's. Nobody else here has an index.
await putCovers('@ann', [{ record: 10, jpeg }]);
await putCovers('@', [{ record: 10, jpeg }]);
await putCovers('@dee', [{ record: 11, jpeg }]);

const gone = () => Promise.reject(new TypeError('no network'));
/** A worker with `who` signed in and no network, or what is given as the network. */
async function signedIn(who: string | null, answer: (path: string) => Promise<Response> = gone) {
  const w = worker();
  await w.tell({ type: 'account', name: who });
  w.net.answer = answer;
  return w;
}
const bytesOf = async (r: Response | undefined) => [...new Uint8Array(await r!.arrayBuffer())];

test('out of reach, a record shows the small cover this device keeps', async () => {
  const r = await (await signedIn('ann')).ask(COVER);
  assert.equal(r!.status, 200);
  assert.equal(r!.headers.get('content-type'), 'image/jpeg');
  assert.deepEqual(await bytesOf(r), [...jpeg]);
});

test("the proxy's refusal is out of reach too", async () => {
  const r = await (await signedIn('ann', async () => new Response('', { status: 403 }))).ask(COVER);
  assert.deepEqual(await bytesOf(r), [...jpeg]);
});

test('Cratarr answering, its cover shows', async () => {
  const r = await (await signedIn('ann', async () => new Response('full'))).ask(COVER);
  assert.equal(await r!.text(), 'full');
});

test("Cratarr's not found stands: it has no cover, whatever is kept from before", async () => {
  const r = await (await signedIn('ann', async () => new Response('', { status: 404 }))).ask(COVER);
  assert.equal(r!.status, 404);
});

test('a full cover kept for what is downloaded goes first', async () => {
  const w = await signedIn('ann');
  await (await w.store.caches.open(`${COVERS}@ann`)).put(COVER, new Response('full'));
  assert.equal(await (await w.ask(COVER))!.text(), 'full');
});

test("a record with no small cover fails as it did, and another account's are not this one's", async () => {
  assert.equal((await (await signedIn('dee')).ask(COVER))!.type, 'error');
});

test("nobody signed in shows nobody's, and asks nobody", async () => {
  const asked: string[] = [];
  const w = worker();
  w.net.answer = (path) => {
    asked.push(path);
    return gone();
  };
  assert.equal((await w.ask(COVER))!.type, 'error');
  assert.deepEqual(asked, [COVER]);
});

test('with sign-in off the one user has theirs', async () => {
  assert.deepEqual(await bytesOf(await (await signedIn('')).ask(COVER)), [...jpeg]);
});

test('a server other than Cratarr has no small covers', async () => {
  assert.equal((await (await signedIn('ann')).ask('/api/albums/jellyfin/al-10/cover'))!.type, 'error');
});

test("looking leaves no index behind: making one is the page's", async () => {
  await (await signedIn('cy')).ask(COVER);
  assert.equal(databases.has('cratarr-index@cy'), false);
});

test('in the app, which keeps no index here, the cover comes from the app', async () => {
  const asked: string[] = [];
  const app = async (path: string) => {
    asked.push(path);
    return path.startsWith('/__device/') ? new Response('from the app', { headers: { 'content-type': 'image/jpeg' } }) : gone();
  };
  const r = await (await signedIn('cy', app)).ask(COVER);
  assert.equal(await r!.text(), 'from the app');
  assert.deepEqual(asked, [COVER, '/__device/index/cover/10']);
});

test('the app holding none, the record fails as it did', async () => {
  const app = async (path: string) => (path.startsWith('/__device/') ? new Response('', { status: 404 }) : gone());
  assert.equal((await (await signedIn('cy', app)).ask(COVER))!.type, 'error');
});

test('a server that answers its page there gives no cover', async () => {
  const server = async (path: string) => (path.startsWith('/__device/') ? new Response('<!doctype html>', { headers: { 'content-type': 'text/html' } }) : gone());
  assert.equal((await (await signedIn('cy', server)).ask(COVER))!.type, 'error');
});

test('a browser with an index does not go on to ask the app', async () => {
  const asked: string[] = [];
  const net = (path: string) => {
    asked.push(path);
    return gone();
  };
  await (await signedIn('dee', net)).ask(COVER);
  assert.deepEqual(asked, [COVER]);
});

test('an index made before covers had a store reads as holding none', async () => {
  databases.set('cratarr-index@old', new Map());
  assert.equal((await (await signedIn('old')).ask(COVER))!.type, 'error');
});

test("a list's size of a cover never takes the place of the whole one kept for the player away from home", async () => {
  const w = await signedIn('ann', async () => new Response('a list size'));
  await (await w.store.caches.open(`${COVERS}@ann`)).put(COVER, new Response('whole'));
  assert.equal(await (await w.ask(`${COVER}?size=160`))!.text(), 'a list size');
  await new Promise((go) => setTimeout(go, 10));
  assert.equal(await (await w.store.caches.match(COVER, { cacheName: `${COVERS}@ann` }))!.text(), 'whole');
});

test('asked whole, a kept cover is kept fresh', async () => {
  const w = await signedIn('ann', async () => new Response('whole, new'));
  await (await w.store.caches.open(`${COVERS}@ann`)).put(COVER, new Response('whole'));
  await w.ask(COVER);
  await new Promise((go) => setTimeout(go, 10));
  assert.equal(await (await w.store.caches.match(COVER, { cacheName: `${COVERS}@ann` }))!.text(), 'whole, new');
});
