// Two accounts on one device, in the page's own stores (decision 0101, points 13 and 14; lib/offline, lib/shelfStore,
// lib/localCovers and lib/scrobbles, the real ones, on stand-ins for IndexedDB and Cache Storage): each account's are
// named for it and hidden while another is signed in, a stream two keep is stored once, and work still going as one
// account leaves ends up as that one's, never the next one's.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { setTimeout as wait } from 'node:timers/promises';
import { effects } from '../harness/pageGlobals.ts';
import { databases, rowsOf } from '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
// With a window the page's pollers start: as its timers (tests/harness/pageGlobals), they keep no test running.
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: true, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
// A cover made small, as far as the shelf asks.
g.createImageBitmap = async () => ({ width: 10, height: 10, close() {} });
g.HTMLCanvasElement = class {};
g.OffscreenCanvas = class {
  getContext = () => ({ drawImage() {} });
  convertToBlob = async ({ type }: { type: string }) => new Blob(['small'], { type });
};
g.FileReader = class {
  result: string | null = null;
  onload: (() => void) | null = null;
  readAsDataURL(b: Blob) {
    void b.text().then((t) => ((this.result = `data:image/jpeg;base64,${btoa(t)}`), this.onload?.()));
  }
};
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

/** Cache Storage: each cache's keys, and what is in them. */
const cached = new Map<string, Map<string, Response>>();
const cacheOf = (n: string) => {
  const c = cached.get(n) ?? new Map<string, Response>();
  cached.set(n, c);
  return {
    put: async (k: string, r: Response) => void c.set(String(k), r),
    delete: async (k: string) => c.delete(String(k)),
  };
};
g.caches = { open: async (n: string) => cacheOf(n) };

const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
/** The tracks asked to be kept come back as ready, and their files, covers and waveforms as bytes. `held`: the asks for
 *  these addresses wait for `go`. */
let held: RegExp | null = null;
let go: () => void = () => {};
function holdBack(addresses: RegExp) {
  const waiting = new Promise<void>((done) => (go = done));
  held = addresses;
  return waiting;
}
let waiting: Promise<void> = Promise.resolve();
let asked = 0;
/** Every address asked. */
const addresses: string[] = [];
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  addresses.push(path);
  if (held?.test(path)) {
    asked++;
    await waiting;
  }
  if (path === '/api/offline/prepare' || path === '/api/offline') {
    const tracks = JSON.parse(String(init?.body)).tracks.map((t: { source: string; id: string }) => ({ ...t, offline: true, file: { url: `/file/${t.id}`, type: 'audio/ogg', ready: true, bytes: 3 }, cover: `/cover/${t.id}`, waveform: `/wave/${t.id}`, track: { duration: 60 } }));
    return json({ tracks });
  }
  if (path.startsWith('/api/albums/')) return json({ title: 'A record', artist: 'Someone', year: 2001, genres: [], rows: [] });
  return new Response('abc');
}) as typeof fetch;

const { accountForgotten, accountLearnt } = await import('../../src/lib/account.ts');
const { forgetOffline, isKept, keepOffline, offline } = await import('../../src/lib/offline.svelte.ts');
const { shelf } = await import('../../src/lib/shelfStore.svelte.ts');
const { chooseHomePicture, homePicture, localCovers, setLocalCover } = await import('../../src/lib/localCovers.svelte.ts');
const { readHere, scrobbling, send } = await import('../../src/lib/scrobbles.svelte.ts');
const { mine } = await import('../../src/lib/mySettings.svelte.ts');
const { exporting } = await import('../../src/lib/export.svelte.ts');
const { later, waitingHere } = await import('../../src/lib/later.svelte.ts');

const TRACK = { source: 'cratarr', id: 'tr-1', name: 'One', artist: 'Someone', album: 'A record', album_id: 'al-1' };
const stream = '/api/stream/cratarr/tr-1';
/** IndexedDB's and the stand-ins' next turns: what was started has finished, on a clock that ticks every 15 ms. */
const turns = () => wait(80);
/** Until `done` says so, a few seconds at most. */
async function until(done: () => boolean) {
  for (let i = 0; i < 400 && !done(); i++) await wait(5);
}
const follow = () => {
  for (const run of effects) run();
};

test('an installed page keeps each account its own list, on the names W7 raised, and the old list goes', async () => {
  accountLearnt('ann');
  await turns();
  await keepOffline([TRACK]);
  assert.ok(isKept(TRACK));
  const list = JSON.parse(localStorage.getItem('cratarr-offline-index-2@ann')!);
  assert.deepEqual(Object.keys(list), ['cratarr/tr-1']);
  assert.equal(localStorage.getItem('cratarr-offline-index-2'), null, 'nothing under a name for no one');
  assert.ok(cached.get('cratarr-offline-2')?.has(stream), 'the stream, everyone\'s');
  assert.ok(cached.get('cratarr-covers-2@ann')?.has('/cover/tr-1'), 'its cover, hers');
  assert.ok(cached.get('cratarr-data-2@ann')?.has('/wave/tr-1'), 'its waveform, hers');
});

test('another account has none of it, nor has nobody, and hers is there at her return', async () => {
  accountLearnt('bo');
  await turns();
  assert.equal(offline.who, '@bo');
  assert.deepEqual(offline.kept, {});
  assert.equal(isKept(TRACK), false);
  accountForgotten();
  await turns();
  assert.deepEqual(offline.kept, {});
  accountLearnt('ann');
  await turns();
  assert.ok(isKept(TRACK));
});

test('a stream two accounts keep is stored once, and goes with the last one to let go', async () => {
  accountLearnt('bo');
  await turns();
  await keepOffline([TRACK]);
  assert.equal([...cached.get('cratarr-offline-2')!.keys()].filter((k) => k === stream).length, 1);
  accountLearnt('ann');
  await turns();
  await forgetOffline([TRACK]);
  assert.equal(isKept(TRACK), false);
  assert.ok(cached.get('cratarr-offline-2')!.has(stream), 'he keeps it still');
  accountLearnt('bo');
  await turns();
  assert.ok(isKept(TRACK));
  await forgetOffline([TRACK]);
  assert.equal(cached.get('cratarr-offline-2')!.has(stream), false);
});

test('a keep that outlives its account stores nothing with no list, and gives the next one none of what was left of it', async () => {
  accountLearnt('ann');
  await turns();
  const first = { ...TRACK, id: 'tr-first' };
  const next = { ...TRACK, id: 'tr-next' };
  const streams = () => [...(cached.get('cratarr-offline-2')?.keys() ?? [])].filter((k) => /tr-(first|next)/.test(k));
  waiting = holdBack(/\/file\/tr-first/);
  const keeping = keepOffline([first, next]);
  while (!asked) await turns();
  accountLearnt('bo');
  await turns();
  go();
  await keeping;
  held = null;
  assert.deepEqual(streams(), [], 'it would sit there for ever, in nobody’s list');
  assert.equal(isKept(first), false);
  assert.equal(isKept(next), false, 'not his');
  assert.ok(!addresses.includes('/file/tr-next'), 'nor fetched under his session');
  accountLearnt('ann');
  await turns();
  assert.equal(isKept(next), false);
  asked = 0;
});

test("the shelf is each account's own database, following only that account's list", async () => {
  accountLearnt('ann');
  await turns();
  await keepOffline([TRACK]);
  follow();
  await turns();
  assert.equal(shelf.who, '@ann');
  assert.deepEqual(Object.keys(shelf.tracks), ['browser:cratarr/tr-1']);
  assert.ok(rowsOf('cratarr-shelf@ann', 'tracks').has('browser:cratarr/tr-1'));
  accountLearnt('bo');
  await turns();
  assert.deepEqual(shelf.tracks, {}, 'gone by itself, before anything says what he holds');
  follow();
  await turns();
  assert.equal(shelf.who, '@bo');
  assert.deepEqual(shelf.tracks, {}, 'none of hers for him');
  assert.deepEqual(shelf.covers, {});
  assert.equal(rowsOf('cratarr-shelf@bo', 'tracks').size, 0);
  assert.ok(rowsOf('cratarr-shelf@ann', 'tracks').has('browser:cratarr/tr-1'), 'and hers left as it was');
  assert.equal(databases.has('cratarr-shelf'), false, 'none under a name for no one');
});

/** ann's shelf filling album `album`, held at the ask for `addresses` until bo has signed in, then let go. */
async function fillAcrossAChange(album: string, addresses: RegExp) {
  accountLearnt('ann');
  await turns();
  await keepOffline([{ ...TRACK, id: `t-${album}`, album_id: album }]);
  asked = 0;
  waiting = holdBack(addresses);
  follow();
  await until(() => asked > 0);
  assert.ok(asked > 0, 'the fill has come to the ask held back');
  accountLearnt('bo');
  await turns();
  follow();
  held = null;
  go();
  await turns();
}
const covers = (who: string) => [...rowsOf(`cratarr-shelf@${who}`, 'covers').keys()];

test("an album's view on its way as its account goes: nothing of it in the next one's shelf, and its cover is not read for it", async () => {
  const before = covers('bo');
  await fillAcrossAChange('al-2', /^\/api\/albums\/cratarr\/al-2(\?|$)/);
  assert.deepEqual(covers('bo'), before);
  assert.deepEqual(Object.keys(shelf.covers), []);
  assert.equal(rowsOf('cratarr-shelf@bo', 'tracks').size, 0);
  assert.deepEqual(addresses.filter((a) => a.includes('/al-2/cover')), [], 'it stops there, not reading hers under his session');
});

test("a cover on its way as its account goes is not kept in the next one's shelf", async () => {
  const before = covers('bo');
  await fillAcrossAChange('al-3', /\/al-3\/cover/);
  assert.deepEqual(covers('bo'), before, 'no cover of hers in his');
  assert.deepEqual(Object.keys(shelf.covers), []);
});

test("the next one's shelf is filled once the last one's has stopped, with nothing else to set it going", async () => {
  // A record of his, waiting for its details: come from an earlier visit, with no change of his list to follow.
  rowsOf('cratarr-shelf@bo', 'tracks').set('browser:cratarr/t-9', { key: 'browser:cratarr/t-9', where: 'browser', source: 'cratarr', id: 't-9', name: 'His', artist: null, album: null, album_id: 'al-9', album_artist: null, year: null, duration_secs: null, genres: [], disc: null, track: null, added: 1, filled: 0 });
  accountLearnt('ann');
  await turns();
  await keepOffline([{ ...TRACK, id: 't-al-4', album_id: 'al-4' }]);
  asked = 0;
  waiting = holdBack(/\/al-4/);
  follow();
  await until(() => asked > 0);
  accountLearnt('bo');
  await turns();
  held = null;
  go();
  await until(() => covers('bo').includes('cratarr/al-9'));
  assert.ok(covers('bo').includes('cratarr/al-9'));
  rowsOf('cratarr-shelf@bo', 'tracks').delete('browser:cratarr/t-9');
  rowsOf('cratarr-shelf@bo', 'covers').delete('cratarr/al-9');
  accountForgotten();
  await turns();
});

test("a cover chosen here is the account's, in its own database", async () => {
  accountLearnt('ann');
  await turns();
  await setLocalCover('cratarr', 'al-1', new Blob(['hers']));
  await turns();
  assert.ok(localCovers['cratarr/al-1']);
  assert.ok(rowsOf('cratarr-covers@ann', 'covers').has('cratarr/al-1'));
  accountLearnt('bo');
  await turns();
  assert.equal(localCovers['cratarr/al-1'], undefined);
  accountForgotten();
  await turns();
  assert.equal(localCovers['cratarr/al-1'], undefined);
  accountLearnt('ann');
  await turns();
  assert.ok(localCovers['cratarr/al-1'], 'hers again at her return');
});

test('a picture still being read as its account goes is kept as nobody\'s else', async () => {
  accountLearnt('ann');
  await turns();
  const chosen = setLocalCover('cratarr', 'al-2', new Blob(['hers too']));
  accountLearnt('bo');
  await chosen;
  await turns();
  assert.equal(localCovers['cratarr/al-2'], undefined);
  assert.equal(rowsOf('cratarr-covers@bo', 'covers').size, 0);
});

test("Home's picture still being made as its account goes is held by no one else", async () => {
  accountLearnt('ann');
  await turns();
  await chooseHomePicture(new Blob(['hers']));
  assert.ok(homePicture.held, 'chosen with nothing in the way, it is held');
  const chosen = chooseHomePicture(new Blob(['hers, later']));
  await wait(1);
  accountLearnt('bo');
  await chosen;
  await turns();
  assert.equal(homePicture.held, null);
  assert.equal(localStorage.getItem('cratarr-home-picture@bo'), null);
});

test('what waits to be sent shows to its own account alone: another one sees neither how much nor what', async () => {
  accountLearnt('ann');
  await turns();
  const change = (key: string, account?: string) => ({ key, at: Date.now(), path: '/api/playlists/pl-1/ops', method: 'POST', body: '{"ops":[]}', account });
  const was = later.list;
  later.list = [change('hers', 'ann'), change('his', 'bo')];
  assert.deepEqual(waitingHere().map((c) => c.key), ['hers']);
  accountLearnt('bo');
  assert.deepEqual(waitingHere().map((c) => c.key), ['his']);
  accountForgotten();
  assert.deepEqual(waitingHere(), [], 'nobody sees none');
  later.list = was;
  // The two places that show what waits: read from their source, which says a line is there, not that it works.
  for (const file of ['../../src/App.svelte', '../../src/lib/PlaylistView.svelte']) {
    const source = readFileSync(new URL(file, import.meta.url), 'utf8');
    assert.match(source, /waitingHere\(\)/, file);
    assert.doesNotMatch(source, /later\.list/, file);
  }
});

test("what an export was named is the account's: it follows her playlists' names, and is not to be read by the next", async () => {
  accountLearnt('ann');
  await turns();
  exporting.name = 'Her road trip';
  exporting.named = true;
  accountLearnt('bo');
  await turns();
  assert.equal(exporting.name, '', 'none of hers for him');
  assert.equal(exporting.named, false);
  exporting.name = 'His';
  accountForgotten();
  await turns();
  assert.equal(exporting.name, '', 'nor for nobody');
  accountLearnt('ann');
  await turns();
  assert.equal(exporting.name, 'Her road trip');
  assert.equal(exporting.named, true);
  assert.equal(localStorage.getItem('cratarr-export'), null, 'nothing under a name for no one');
  accountLearnt('bo');
  await turns();
  assert.equal(exporting.name, 'His', 'and his is his');
});

test('scrobbling sign-ins and what waits are each account\'s here', async () => {
  accountLearnt('ann');
  await turns();
  localStorage.setItem('cratarr.scrobblers@ann', JSON.stringify({ listenbrainz: { token: 'hers', name: 'ann-lb' } }));
  localStorage.setItem('cratarr.scrobbleQueue@ann', JSON.stringify({ plays: [{ id: 'p1', at: 1, artist: 'a', title: 't', album: null }], sent: { listenbrainz: [], lastfm: [] } }));
  readHere();
  assert.equal(scrobbling.here.listenbrainz?.name, 'ann-lb');
  assert.equal(scrobbling.queued, 1);
  accountLearnt('bo');
  await turns();
  assert.equal(scrobbling.here.listenbrainz, null, 'none of hers for him');
  assert.equal(scrobbling.queued, 0);
  accountForgotten();
  await turns();
  assert.equal(scrobbling.here.listenbrainz, null);
  accountLearnt('ann');
  await turns();
  assert.equal(scrobbling.here.listenbrainz?.name, 'ann-lb');
});

/** A send of ann's held at ListenBrainz until the account has become bo's, then answered with `answer`. */
async function sendAcrossAChange(answer: Response) {
  accountLearnt('ann');
  await turns();
  mine.tracking = true;
  localStorage.setItem('cratarr.scrobblers@ann', JSON.stringify({ listenbrainz: { token: 'hers', name: 'ann-lb' } }));
  localStorage.setItem('cratarr.scrobbleQueue@ann', JSON.stringify({ plays: [{ id: 'hers', at: 1, artist: 'a', title: 't', album: null }], sent: { listenbrainz: [], lastfm: [] } }));
  localStorage.removeItem('cratarr.scrobblers@bo');
  let go: (r: Response) => void = () => {};
  const lb = new Promise<Response>((done) => (go = done));
  const before = globalThis.fetch;
  globalThis.fetch = (async (input: string, init?: RequestInit) => (String(input).includes('listenbrainz') ? lb : before(input, init))) as typeof fetch;
  const sending = send();
  await wait(1);
  accountLearnt('bo');
  await turns();
  localStorage.setItem('cratarr.scrobbleQueue@bo', JSON.stringify({ plays: [{ id: 'his', at: 2, artist: 'b', title: 'u', album: null }], sent: { listenbrainz: [], lastfm: [] } }));
  go(answer);
  await sending;
  globalThis.fetch = before;
}
const queueOf = (who: string) => JSON.parse(localStorage.getItem(`cratarr.scrobbleQueue@${who}`)!).plays.map((p: { id: string }) => p.id);

test("a send still going as its account goes leaves the next one's queue as it was, and its own told as sent", async () => {
  await sendAcrossAChange(new Response('{"status":"ok"}'));
  assert.deepEqual(queueOf('bo'), ['his']);
  assert.deepEqual(queueOf('ann'), [], 'what went is not sent again at her return');
});

test("a sign-in refused as its account goes is never written under the next one's name", async () => {
  await sendAcrossAChange(new Response('{}', { status: 401 }));
  assert.equal(localStorage.getItem('cratarr.scrobblers@bo'), null, 'her token is not his');
  assert.deepEqual(queueOf('bo'), ['his']);
  assert.ok(JSON.parse(localStorage.getItem('cratarr.scrobblers@ann')!).listenbrainz.broken, 'she is told at her return');
});
