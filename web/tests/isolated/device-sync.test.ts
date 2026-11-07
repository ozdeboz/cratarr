// Sync on the page (lib/deviceSync, the real one, with the real index.wasm and the rules crate's own files, on a stand-in for Cratarr
// and a fake IndexedDB): what Cratarr was asked, and what the device then holds, to the byte.
import { afterEach, beforeEach, mock, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';
import { rowsOf, storage } from '../harness/fakeIdb.ts';

const connection = { type: 'wifi', saveData: false };
let persistAsked = 0;
Object.defineProperty(globalThis, 'navigator', { value: { connection, storage: { persist: async () => (persistAsked++, true) } }, configurable: true });

const { accountForgotten, accountLearnt, whoOf } = await import('../../src/lib/account.ts');
const { closeReads, deviceAlbums, newEngine } = await import('../../src/lib/deviceIndex.svelte.ts');
const { indexWord } = await import('../../src/lib/deviceIndexRule.ts');
const { ANEW_AFTER_MS, answerMobile, FIRST_LOOK_MS, indexStands, indexSync, lookDone, startAnew, stopLooking, syncNow, tryIndexAgain } = await import('../../src/lib/deviceSync.svelte.ts');
const { readPart, writePart } = await import('../../src/lib/indexStore.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { networkSaid, reached, refused, unreached } = await import('../../src/lib/reach.svelte.ts');
const { LOOK_AGAIN_MS } = await import('../../src/lib/deviceIndexRule.ts');

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const wasm = readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url));

/** Bytes as another version of the format wrote them: the version sits in the header, read before anything else. */
const otherVersion = (bytes: Uint8Array) => {
  const out = bytes.slice();
  out[4] = 9;
  return out;
};

const bytes = (b: Uint8Array) => new Response(b, { headers: { 'content-type': 'application/octet-stream' } });
const none = () => new Response(null, { status: 204 });
const startAgain = () => new Response(JSON.stringify({ rev: 9, reset: true }), { status: 410 });
const lost = () => {
  throw new TypeError('Failed to fetch');
};

/** How Cratarr answers each address, the last named first. */
let answers: Record<string, () => Response> = {};
let asked: string[] = [];
const route = (url: string) => {
  const path = url.split('?')[0];
  if (answers[path]) return answers[path]();
  if (path === '/api/index') return bytes(vector('whole.bin'));
  if (path === '/api/index/sounds') return bytes(vector('sounds.bin'));
  return none();
};
globalThis.fetch = (async (input: string) => {
  const url = String(input);
  if (url.includes('index.wasm')) return new Response(wasm);
  asked.push(url);
  return route(url);
}) as typeof fetch;

let n = 0;
let me = '';
/** Each test's own day: a refusal from one is long over at the next. */
const DAY = 86_400_000;
const kept = (name: 'index' | 'sounds') => readPart(whoOf(me), name);
const same = (a: Uint8Array | null | undefined, b: Uint8Array) => assert.deepEqual(a && [...a], [...b]);

/** `ms` go by, and a look they bring ends. */
async function gone(ms: number) {
  mock.timers.tick(ms);
  await lookDone();
}

/** A start, its first seconds gone. */
async function started() {
  startAnew();
  await gone(FIRST_LOOK_MS);
}

/** `whole.bin` and `sounds.bin` kept, as a first sync leaves them. */
async function held() {
  await started();
  asked = [];
}

beforeEach(() => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 2_000_000_000 + n * DAY });
  [answers, asked, persistAsked] = [{}, [], 0];
  storage.refused = false;
  Object.assign(connection, { type: 'wifi', saveData: false });
  Object.assign(prefs, { indexMobile: 'unasked', storageAsked: false });
  me = `who${++n}`;
  accountLearnt(me);
  Object.assign(indexSync, { at: null, rev: null, shows: 'idle' });
});
// Its timers cleared first, this one and the look for Cratarr: one from before a reset is one the mock cannot clear.
afterEach(async () => {
  await lookDone();
  stopLooking();
  reached();
  mock.timers.reset();
});

test('nothing is fetched in the first seconds of a start, and then it goes', async () => {
  startAnew();
  await syncNow();
  await gone(FIRST_LOOK_MS - 1);
  assert.deepEqual(asked, []);
  await gone(1);
  assert.deepEqual(asked, ['/api/index', '/api/index/sounds']);
});

test('the page\'s own first seconds and hours are the rule\'s', async () => {
  const e = await newEngine();
  assert.equal(FIRST_LOOK_MS, e.firstAfter());
  assert.equal(ANEW_AFTER_MS, e.anewAfter());
});

test('whole once: the bytes kept as fetched, each part at its rev, and This device has the date', async () => {
  await started();
  same((await kept('index'))?.bytes, vector('whole.bin'));
  same((await kept('sounds'))?.bytes, vector('sounds.bin'));
  assert.equal((await kept('index'))?.rev, 5);
  assert.equal((await kept('sounds'))?.rev, 5);
  assert.equal(indexSync.at, Date.now());
  assert.equal(indexSync.rev, 5);
  assert.equal(indexWord(indexSync.shows, indexSync.at, () => 'the date'), 'the date');
});

test('persistent storage is asked for at the first download, and once', async () => {
  await started();
  assert.equal(persistAsked, 1, 'once, for the two parts');
  me = `who${++n}`;
  accountLearnt(me);
  await started();
  assert.equal(persistAsked, 1, 'the device was asked: another account is no first download');
});

test('with a whole held, a look asks only for what changed', async () => {
  await held();
  mock.timers.tick(1);
  await syncNow();
  assert.deepEqual(asked, ['/api/index/changes?since=5', '/api/index/sounds/changes?since=5']);
});

test('a new start over a copy this page\'s version kept asks only for what changed', async () => {
  await held();
  await started();
  assert.deepEqual(asked, ['/api/index/changes?since=5', '/api/index/sounds/changes?since=5']);
});

test('a feed applied leaves what a fresh fetch would, to the byte, and moves the date', async () => {
  await held();
  mock.timers.tick(60_000);
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  answers['/api/index/sounds/changes'] = () => bytes(vector('sound-changes.bin'));
  await syncNow();
  same((await kept('index'))?.bytes, vector('after.bin'));
  same((await kept('sounds'))?.bytes, vector('sounds-after.bin'));
  assert.equal((await kept('index'))?.rev, 9);
  assert.equal(indexSync.rev, 9);
  assert.equal(indexSync.at, Date.now());
});

test('nothing changed moves the date and leaves the bytes', async () => {
  await held();
  mock.timers.tick(60_000);
  await syncNow();
  same((await kept('index'))?.bytes, vector('whole.bin'));
  assert.equal(indexSync.at, Date.now());
});

test('on mobile data the whole is asked first, nothing fetched till then', async () => {
  connection.type = 'cellular';
  await started();
  assert.deepEqual(asked, []);
  assert.equal(indexSync.shows, 'asks');
  await answerMobile(true);
  assert.deepEqual(asked, ['/api/index', '/api/index/sounds']);
  assert.equal(indexSync.shows, 'idle');
});

test('a browser saving data counts as mobile data', async () => {
  connection.saveData = true;
  await started();
  assert.equal(indexSync.shows, 'asks');
});

test('a no waits for Wi-Fi, asks no more, and This device can ask again', async () => {
  connection.type = 'cellular';
  await started();
  answerMobile(false);
  assert.equal(indexSync.shows, 'wifi');
  await syncNow();
  assert.deepEqual(asked, []);
  assert.equal(indexSync.shows, 'wifi', 'not asked again');
  await tryIndexAgain();
  assert.equal(indexSync.shows, 'asks');
  answerMobile(false);
  connection.type = 'wifi';
  await syncNow();
  assert.deepEqual(asked, ['/api/index', '/api/index/sounds']);
});

test('changes go on mobile data with nothing asked', async () => {
  await held();
  connection.type = 'cellular';
  mock.timers.tick(1);
  await syncNow();
  assert.deepEqual(asked, ['/api/index/changes?since=5', '/api/index/sounds/changes?since=5']);
  assert.equal(indexSync.shows, 'idle');
});

test('"start again" fetches it whole at once, on Wi-Fi', async () => {
  await held();
  answers['/api/index/changes'] = startAgain;
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow();
  assert.deepEqual(asked.slice(0, 2), ['/api/index/changes?since=5', '/api/index']);
  same((await kept('index'))?.bytes, vector('after.bin'));
  assert.equal(indexSync.rev, 9);
});

test('"start again" on mobile data is a whole like any: asked, and what is held stays readable', async () => {
  await held();
  connection.type = 'cellular';
  answers['/api/index/changes'] = startAgain;
  await syncNow();
  assert.equal(indexSync.shows, 'asks');
  same((await kept('index'))?.bytes, vector('whole.bin'));
});

test('a lost connection is no failure: it waits, says nothing, and looks again', async () => {
  await held();
  answers['/api/index/changes'] = lost;
  await syncNow();
  const part = (await kept('index'))?.part;
  assert.equal(part?.lost, 1);
  assert.equal(part?.failed, 0);
  assert.equal(indexSync.shows, 'idle');
  const before = asked.length;
  await syncNow();
  assert.equal(asked.filter((a) => a.startsWith('/api/index/changes')).length, before - 1, 'not before its wait');
});

test('a proxy\'s answer for a stopped Cratarr is a lost connection, not a failure', async () => {
  await held();
  answers['/api/index/changes'] = () => new Response('', { status: 502 });
  await syncNow();
  assert.equal((await kept('index'))?.part.lost, 1);
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('lost a few times in a row it says paused, and Cratarr back ends that', async () => {
  await held();
  answers['/api/index/changes'] = lost;
  await syncNow();
  await gone(30_000);
  assert.equal(indexSync.shows, 'idle', 'twice is not yet');
  await gone(30_000);
  assert.equal(indexSync.shows, 'paused');
  assert.equal(indexWord(indexSync.shows, indexSync.at, () => 'date'), 'Paused');
  delete answers['/api/index/changes'];
  await gone(30_000);
  assert.equal(indexSync.shows, 'idle');
});

test('a real failure counts, waits longer, and a start forgets it', async () => {
  await held();
  answers['/api/index/changes'] = () => new Response('', { status: 500 });
  await syncNow();
  assert.equal((await kept('index'))?.part.failed, 1);
  const calls = () => asked.filter((a) => a.startsWith('/api/index/changes')).length;
  const once = calls();
  await gone(30_000);
  assert.equal(calls(), once, 'a minute, the first time');
  await gone(30_000);
  assert.equal(calls(), once + 1);
  assert.equal((await kept('index'))?.part.failed, 2);
  delete answers['/api/index/changes'];
  await started();
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('bytes cut short count as a failure, and nothing of them is kept', async () => {
  answers['/api/index'] = () => bytes(vector('whole.bin').slice(0, 100));
  await started();
  const part = await kept('index');
  assert.equal(part?.bytes, null);
  assert.equal(part?.part.failed, 1);
  assert.equal(indexSync.at, null);
});

test('another version in bytes just fetched stops, and fetches no more, at the next look or the next start', async () => {
  answers['/api/index'] = () => bytes(otherVersion(vector('whole.bin')));
  await started();
  assert.equal(indexSync.shows, 'stopped');
  assert.equal(indexWord(indexSync.shows, indexSync.at, () => 'date'), 'Differs from Cratarr');
  const fetches = () => asked.filter((a) => a === '/api/index').length;
  assert.equal(fetches(), 1);
  await gone(60_000);
  await syncNow();
  await started();
  assert.equal(fetches(), 1);
  assert.equal(indexSync.shows, 'stopped');
});

test('a stop is this page\'s: one that reads another version tries again', async () => {
  answers['/api/index'] = () => bytes(otherVersion(vector('whole.bin')));
  await started();
  const stopped = (await kept('index'))!;
  await writePart(whoOf(me), 'index', { ...stopped, wants: 0 });
  delete answers['/api/index'];
  await started();
  same((await kept('index'))?.bytes, vector('whole.bin'));
  assert.equal(indexSync.shows, 'idle');
});

test('a tap on what stopped tries again', async () => {
  answers['/api/index'] = () => bytes(otherVersion(vector('whole.bin')));
  await started();
  delete answers['/api/index'];
  await tryIndexAgain();
  same((await kept('index'))?.bytes, vector('whole.bin'));
});

test('bytes held of another version are not a stop: they are fetched whole', async () => {
  await held();
  const stands = (await kept('index'))!;
  await writePart(whoOf(me), 'index', { ...stands, bytes: otherVersion(vector('whole.bin')) });
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow();
  same((await kept('index'))?.bytes, vector('after.bin'));
  assert.equal(indexSync.shows, 'idle');
});

test('a copy a page of another version kept is fetched whole, though the feed has nothing to say', async () => {
  await held();
  const stands = (await kept('index'))!;
  await writePart(whoOf(me), 'index', { ...stands, bytes: otherVersion(vector('whole.bin')), wants: 0 });
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await started();
  same((await kept('index'))?.bytes, vector('after.bin'));
  assert.equal(indexSync.rev, 9);
  assert.equal(indexSync.shows, 'idle');
});

test('a feed that does not fit is no failure either: it is fetched whole', async () => {
  await held();
  answers['/api/index/changes'] = () => bytes(vector('changes.bin').slice(0, 50));
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow();
  same((await kept('index'))?.bytes, vector('after.bin'));
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('a feed with another version in it asks for the whole, and only that can stop it', async () => {
  await held();
  answers['/api/index/changes'] = () => bytes(otherVersion(vector('changes.bin')));
  await syncNow();
  assert.ok(asked.includes('/api/index'));
  assert.equal(indexSync.shows, 'idle');
});

test('storage refusing what was fetched shows no date, waits as a failure does, and a later fetch keeps it', async () => {
  storage.refused = true;
  await started();
  assert.equal(indexSync.at, null);
  assert.equal(indexSync.rev, null);
  assert.equal(indexWord(indexSync.shows, indexSync.at, () => 'the date'), 'Not yet');
  const fetches = () => asked.filter((a) => a === '/api/index').length;
  assert.equal(fetches(), 1);
  await syncNow();
  assert.equal(fetches(), 1, 'not again at once');
  storage.refused = false;
  await gone(60_000);
  assert.equal(fetches(), 2);
  same((await kept('index'))?.bytes, vector('whole.bin'));
  assert.equal(indexSync.at, Date.now());
});

test('each account\'s lies apart, and what was fetched for one is not kept for another', async () => {
  let openDoor = () => {};
  answers['/api/index'] = () => {
    openDoor();
    return bytes(vector('after.bin'));
  };
  const ann = me;
  openDoor = () => accountLearnt('someone-else');
  await started();
  assert.equal((await readPart(whoOf('someone-else'), 'index'))?.bytes ?? null, null);
  assert.equal((await readPart(whoOf(ann), 'index'))?.bytes ?? null, null);
});

test('stores are named for their account', async () => {
  await held();
  assert.ok(rowsOf(`cratarr-index${whoOf(me)}`, 'parts').has('index'));
});

test('nobody signed in fetches nothing', async () => {
  accountForgotten();
  await started();
  assert.deepEqual(asked, []);
});

test('a refusal holding the requests back holds this too', async () => {
  refused();
  await started();
  assert.deepEqual(asked, []);
});

test('with reads on the device nothing is fetched, and nothing counts as lost', async () => {
  await held();
  networkSaid(false);
  await syncNow();
  assert.deepEqual(asked, []);
  assert.equal((await kept('index'))?.part.lost, 0);
});

test('Cratarr settled out of reach is the same: no knocking at what will not answer', async () => {
  await held();
  answers['/api/health'] = lost;
  unreached();
  await gone(LOOK_AGAIN_MS);
  await syncNow();
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/index')), []);
  assert.equal((await kept('index'))?.part.lost, 0);
});

test('a session over is no failure, and a page gone to its sign-in will say so', async () => {
  await held();
  answers['/api/index/changes'] = () => new Response('', { status: 401 });
  await syncNow();
  assert.equal((await kept('index'))?.part.lost, 1);
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('a start again answered to a whole is a failure: it cannot be asked for anything less', async () => {
  answers['/api/index'] = startAgain;
  await started();
  assert.equal((await kept('index'))?.part.failed, 1);
});

test('the answers to the asking are kept with the device\'s settings', async () => {
  const { DEFAULTS, fromSaved } = await import('../../src/lib/prefs.svelte.ts');
  assert.equal(DEFAULTS.indexMobile, 'unasked');
  assert.equal(DEFAULTS.storageAsked, false);
  assert.equal(fromSaved('indexMobile', 'no'), 'no');
  assert.equal(fromSaved('storageAsked', true), true);
  assert.equal(fromSaved('indexMobile', 'perhaps'), 'unasked');
});

test('This device says each state, and the date when nothing stands in the way', () => {
  const said = (shows: Parameters<typeof indexWord>[0], at: number | null) => indexWord(shows, at, () => 'the date');
  assert.equal(said('idle', null), 'Not yet');
  assert.equal(said('fetching', null), 'Fetching');
  assert.equal(said('fetching', 1), 'the date', 'a look under way is no news');
  assert.equal(said('idle', 1), 'the date');
  assert.equal(said('wifi', null), 'Waiting for Wi-Fi');
  assert.equal(said('asks', null), 'Waiting for Wi-Fi');
  assert.equal(said('paused', 1), 'Paused');
  assert.equal(said('stopped', 1), 'Differs from Cratarr');
});

test('what /api/changes says of the library starts a look; nothing moved starts none', async () => {
  await held();
  await indexStands({ changed: false });
  await indexStands(undefined);
  assert.deepEqual(asked, []);
  await indexStands({ changed: true });
  assert.ok(asked.includes('/api/index/changes?since=5'));
});

test('a sync keeps nothing open: a read away opens what is kept now, and the sounds stay in their bytes', async () => {
  await held();
  closeReads();
  networkSaid(false);
  assert.equal((await deviceAlbums())?.sources[0].albums.length, 3);
  reached();
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  await syncNow();
  networkSaid(false);
  assert.equal((await deviceAlbums())?.sources[0].albums.length, 4);
  reached();
});
