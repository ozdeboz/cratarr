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
const { readBytes, readPart, writePart } = await import('../../src/lib/indexStore.ts');
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

/** Small covers as the server writes them (`CRXV`): the form is plain, and the crate has no writer of it to read from. */
const joined = (...parts: Uint8Array[]) => new Uint8Array(parts.flatMap((p) => [...p]));
const u32 = (n: number) => new Uint8Array(new Uint32Array([n]).buffer);
const i64 = (n: number) => new Uint8Array(new BigInt64Array([BigInt(n)]).buffer);
const head = (rev: number, version = 1) => joined(new TextEncoder().encode('CRXV'), u32(version), i64(rev));
const frame = (record: number, jpeg = new Uint8Array(0)) => joined(i64(record), u32(jpeg.length), jpeg);
const picture = (n: number) => new Uint8Array(40).fill(n);
const bundle = (rev: number, ...frames: Uint8Array[]) => joined(head(rev), ...frames);
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
  if (path === '/api/index/covers') return bytes(bundle(5, frame(1, picture(1)), frame(2, picture(2))));
  if (path === '/api/index/own') return bytes(vector('own.bin'));
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
const kept = (name: 'index' | 'sounds' | 'covers' | 'own') => readPart(whoOf(me), name);
const bytesKept = (name: 'index' | 'sounds') => readBytes(whoOf(me), name);
const coversKept = () => rowsOf(`cratarr-index${whoOf(me)}`, 'covers');
const WHOLES = ['/api/index', '/api/index/sounds', '/api/index/covers', '/api/index/own'];
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

/** `whole.bin`, `sounds.bin` and two covers kept, as a first sync leaves them. */
async function held() {
  await started();
  asked = [];
}

beforeEach(() => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 2_000_000_000 + n * DAY });
  [answers, asked, persistAsked] = [{}, [], 0];
  Object.assign(storage, { refused: false, only: null });
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
  assert.deepEqual(asked, WHOLES);
});

test('the page\'s own first seconds and hours are the rule\'s', async () => {
  const e = await newEngine();
  assert.equal(FIRST_LOOK_MS, e.firstAfter());
  assert.equal(ANEW_AFTER_MS, e.anewAfter());
});

test('whole once: the bytes kept as fetched, each part at its rev, and This device has the date', async () => {
  await started();
  same(await bytesKept('index'), vector('whole.bin'));
  same(await bytesKept('sounds'), vector('sounds.bin'));
  assert.equal((await kept('index'))?.rev, 5);
  assert.equal((await kept('sounds'))?.rev, 5);
  assert.equal(indexSync.at, Date.now());
  assert.equal(indexSync.rev, 5);
  assert.equal(indexWord(indexSync.shows, indexSync.at, () => 'the date'), 'the date');
});

const HOURS = 3_600_000;

test("an account's own goes whole with the rest, and once kept is not asked again for hours", async () => {
  await held();
  same(await readBytes(whoOf(me), 'own'), vector('own.bin'));
  await syncNow(true);
  assert.ok(!asked.some((a) => a.startsWith('/api/index/own')), 'told or not, it has no feed');
  asked = [];
  await gone(6 * HOURS - 1);
  await syncNow();
  assert.ok(!asked.includes('/api/index/own'));
  asked = [];
  await gone(1);
  await syncNow();
  assert.ok(asked.includes('/api/index/own'), 'six hours on a free network');
});

test('on mobile data it goes once a day for one who said yes, and asks nothing of its own', async () => {
  await held();
  connection.type = 'cellular';
  await gone(24 * HOURS);
  await syncNow();
  assert.ok(!asked.includes('/api/index/own'), 'nobody said yes');
  assert.equal(indexSync.shows, 'idle', 'and no question');
  prefs.indexMobile = 'yes';
  await syncNow();
  assert.ok(asked.includes('/api/index/own'));
  asked = [];
  await gone(24 * HOURS - 1);
  await syncNow();
  assert.ok(!asked.includes('/api/index/own'), 'not before the day is over');
});

test("however an account's own fares, the row says nothing of it", async () => {
  answers['/api/index/own'] = lost;
  await started();
  for (let n = 0; n < 4; n++) await gone(30_000);
  assert.equal(indexSync.shows, 'idle');
  assert.equal((await kept('own'))?.part.lost, 5, 'it tried, as any part does');
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
  await syncNow(true);
  assert.deepEqual(asked, ['/api/index/changes?since=5', '/api/index/sounds/changes?since=5', '/api/index/covers/changes?since=5']);
});

test('a new start over a copy this page\'s version kept fetches nothing whole', async () => {
  await held();
  await started();
  assert.deepEqual(asked, ['/api/index/covers/changes?since=5']);
});

test('a look not told asks no feed of rows or sounds, as each is a whole build on the server, and the covers\' at every sync', async () => {
  await held();
  mock.timers.tick(60_000);
  await syncNow();
  assert.deepEqual(asked, ['/api/index/covers/changes?since=5']);
  assert.equal(indexSync.at, (await kept('index'))?.at, 'nothing was asked of the rows');
  assert.equal((await kept('covers'))?.part.failed, 0, 'nothing new is no failure');
  assert.equal((await kept('covers'))?.at, Date.now());
});

test('each feed goes from its own rev, and is never given another\'s', async () => {
  await held();
  mock.timers.tick(1);
  answers['/api/index/covers/changes'] = () => bytes(bundle(12));
  await syncNow();
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  await syncNow(true);
  asked = [];
  delete answers['/api/index/changes'];
  delete answers['/api/index/covers/changes'];
  await syncNow(true);
  assert.deepEqual(asked, ['/api/index/changes?since=9', '/api/index/sounds/changes?since=5', '/api/index/covers/changes?since=12']);
});

test('told while a look is under way, the look after it is told too', async () => {
  await held();
  let told = false;
  answers['/api/index/covers/changes'] = () => {
    if (!told) void syncNow(true);
    told = true;
    return none();
  };
  await syncNow();
  for (let i = 0; i < 50; i++) await new Promise((go) => setImmediate(go));
  await lookDone();
  assert.ok(asked.includes('/api/index/changes?since=5'), 'not lost behind the look before');
});

test('a feed applied leaves what a fresh fetch would, to the byte, and moves the date', async () => {
  await held();
  mock.timers.tick(60_000);
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  answers['/api/index/sounds/changes'] = () => bytes(vector('sound-changes.bin'));
  await syncNow(true);
  same(await bytesKept('index'), vector('after.bin'));
  same(await bytesKept('sounds'), vector('sounds-after.bin'));
  assert.equal((await kept('index'))?.rev, 9);
  assert.equal(indexSync.rev, 9);
  assert.equal(indexSync.at, Date.now());
});

test('nothing changed moves the date and leaves the bytes', async () => {
  await held();
  mock.timers.tick(60_000);
  await syncNow(true);
  same(await bytesKept('index'), vector('whole.bin'));
  assert.equal(indexSync.at, Date.now());
});

test('on mobile data the whole is asked first, nothing fetched till then', async () => {
  connection.type = 'cellular';
  await started();
  assert.deepEqual(asked, []);
  assert.equal(indexSync.shows, 'asks');
  await answerMobile(true);
  assert.deepEqual(asked, WHOLES);
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
  assert.deepEqual(asked, WHOLES);
});

test('changes go on mobile data with nothing asked', async () => {
  await held();
  connection.type = 'cellular';
  mock.timers.tick(1);
  await syncNow(true);
  assert.deepEqual(asked, ['/api/index/changes?since=5', '/api/index/sounds/changes?since=5', '/api/index/covers/changes?since=5']);
  assert.equal(indexSync.shows, 'idle');
});

test('"start again" fetches it whole at once, on Wi-Fi', async () => {
  await held();
  answers['/api/index/changes'] = startAgain;
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow(true);
  assert.deepEqual(asked.slice(0, 2), ['/api/index/changes?since=5', '/api/index']);
  same(await bytesKept('index'), vector('after.bin'));
  assert.equal(indexSync.rev, 9);
});

test('"start again" on mobile data is a whole like any: asked, and what is held stays readable', async () => {
  await held();
  connection.type = 'cellular';
  answers['/api/index/changes'] = startAgain;
  await syncNow(true);
  assert.equal(indexSync.shows, 'asks');
  same(await bytesKept('index'), vector('whole.bin'));
});

test('a lost connection is no failure: it waits, says nothing, and looks again', async () => {
  await held();
  answers['/api/index/changes'] = lost;
  await syncNow(true);
  const part = (await kept('index'))?.part;
  assert.equal(part?.lost, 1);
  assert.equal(part?.failed, 0);
  assert.equal(indexSync.shows, 'idle');
  const feeds = () => asked.filter((a) => a.startsWith('/api/index/changes')).length;
  const before = feeds();
  await syncNow(true);
  assert.equal(feeds(), before, 'not before its wait');
});

test('a proxy\'s answer for a stopped Cratarr is a lost connection, not a failure', async () => {
  await held();
  answers['/api/index/changes'] = () => new Response('', { status: 502 });
  await syncNow(true);
  assert.equal((await kept('index'))?.part.lost, 1);
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('lost a few times in a row it says paused, and Cratarr back ends that', async () => {
  await held();
  answers['/api/index/changes'] = lost;
  await syncNow(true);
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
  await syncNow(true);
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
  await syncNow(true);
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('bytes cut short count as a failure, and nothing of them is kept', async () => {
  answers['/api/index'] = () => bytes(vector('whole.bin').slice(0, 100));
  await started();
  const part = await kept('index');
  assert.equal(await bytesKept('index'), undefined);
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
  same(await bytesKept('index'), vector('whole.bin'));
  assert.equal(indexSync.shows, 'idle');
});

test('a tap on what stopped tries again', async () => {
  answers['/api/index'] = () => bytes(otherVersion(vector('whole.bin')));
  await started();
  delete answers['/api/index'];
  await tryIndexAgain();
  same(await bytesKept('index'), vector('whole.bin'));
});

test('bytes held of another version are not a stop: they are fetched whole', async () => {
  await held();
  const stands = (await kept('index'))!;
  await writePart(whoOf(me), 'index', stands, otherVersion(vector('whole.bin')));
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow(true);
  same(await bytesKept('index'), vector('after.bin'));
  assert.equal(indexSync.shows, 'idle');
});

test('a copy a page of another version kept is fetched whole, though the feed has nothing to say', async () => {
  await held();
  const stands = (await kept('index'))!;
  await writePart(whoOf(me), 'index', { ...stands, wants: 0 }, otherVersion(vector('whole.bin')));
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await started();
  same(await bytesKept('index'), vector('after.bin'));
  assert.equal(indexSync.rev, 9);
  assert.equal(indexSync.shows, 'idle');
});

test('a feed that does not fit is no failure either: it is fetched whole', async () => {
  await held();
  answers['/api/index/changes'] = () => bytes(vector('changes.bin').slice(0, 50));
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow(true);
  same(await bytesKept('index'), vector('after.bin'));
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('a feed with another version in it asks for the whole, and only that can stop it', async () => {
  await held();
  answers['/api/index/changes'] = () => bytes(otherVersion(vector('changes.bin')));
  await syncNow(true);
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
  same(await bytesKept('index'), vector('whole.bin'));
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
  assert.equal(await readBytes(whoOf('someone-else'), 'index'), undefined);
  assert.equal(await readBytes(whoOf(ann), 'index'), undefined);
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
  await syncNow(true);
  assert.deepEqual(asked, []);
  assert.equal((await kept('index'))?.part.lost, 0);
});

test('Cratarr settled out of reach is the same: no knocking at what will not answer', async () => {
  await held();
  answers['/api/health'] = lost;
  unreached();
  await gone(LOOK_AGAIN_MS);
  await syncNow(true);
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/index')), []);
  assert.equal((await kept('index'))?.part.lost, 0);
});

test('a session over is no failure, and a page gone to its sign-in will say so', async () => {
  await held();
  answers['/api/index/changes'] = () => new Response('', { status: 401 });
  await syncNow(true);
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
  await indexStands({ rev: 5, changed: false });
  await indexStands(undefined);
  assert.deepEqual(asked, []);
  await indexStands({ rev: 5, changed: true });
  assert.ok(asked.includes('/api/index/changes?since=5'));
});

test('the log gone past the covers and nothing said of rows is a cover: only its feed is asked', async () => {
  await held();
  await indexStands({ rev: 6, changed: false });
  assert.deepEqual(asked, ['/api/index/covers/changes?since=5']);
});

test('"start again" from the log asks the feeds, whose own 410 says it for each', async () => {
  await held();
  answers['/api/index/changes'] = startAgain;
  await indexStands({ rev: 5, changed: true, reset: true });
  assert.ok(asked.includes('/api/index/changes?since=5'));
  assert.ok(asked.includes('/api/index'));
  assert.ok(!asked.includes('/api/index/sounds'), 'a 410 on rows says nothing of the others');
});

test('whole, covers lie a row each by record, at the rev in their own head', async () => {
  await held();
  assert.deepEqual([...coversKept().keys()].sort(), [1, 2]);
  same(coversKept().get(2) as Uint8Array, picture(2));
  assert.equal((await kept('covers'))?.rev, 5);
  assert.equal(indexSync.covers, 5);
});

test('a feed of covers brings what was made, drops what has none now, and goes on from its own rev', async () => {
  await held();
  mock.timers.tick(1);
  answers['/api/index/covers/changes'] = () => bytes(bundle(7, frame(3, picture(3)), frame(1), frame(2, picture(9))));
  await syncNow();
  assert.deepEqual([...coversKept().keys()].sort(), [2, 3]);
  same(coversKept().get(2) as Uint8Array, picture(9));
  assert.equal((await kept('covers'))?.rev, 7);
  assert.equal((await kept('covers'))?.at, Date.now());
  assert.equal((await kept('index'))?.rev, 5, 'a cover moves no other part');
});

test('a feed of covers with nothing in it moves their rev, and nothing else', async () => {
  await held();
  mock.timers.tick(1);
  answers['/api/index/covers/changes'] = () => bytes(bundle(8));
  await syncNow();
  assert.equal((await kept('covers'))?.rev, 8);
  assert.equal(coversKept().size, 2);
});

test('"start again" on covers fetches them whole, the rev inside them, and what is gone goes', async () => {
  await held();
  answers['/api/index/covers/changes'] = () => new Response(JSON.stringify({ rev: 99, reset: true }), { status: 410 });
  answers['/api/index/covers'] = () => bytes(bundle(20, frame(2, picture(2))));
  await syncNow();
  assert.deepEqual([...coversKept().keys()], [2]);
  assert.equal((await kept('covers'))?.rev, 20, 'not the rev in the answer');
});

test('a whole cut short keeps the covers held before it: they go only once the new ones are all in', async () => {
  await held();
  answers['/api/index/covers/changes'] = startAgain;
  const fresh = bundle(20, frame(2, picture(9)), frame(3, picture(3)));
  answers['/api/index/covers'] = () => bytes(fresh.slice(0, -5));
  await syncNow();
  assert.deepEqual([...coversKept().keys()].sort(), [1, 2], 'what came of it taken, nothing gone for what did not');
  same(coversKept().get(2) as Uint8Array, picture(9));
  assert.equal((await kept('covers'))?.rev, 5);
  answers['/api/index/covers'] = () => bytes(fresh);
  await gone(HOURS);
  await syncNow();
  assert.deepEqual([...coversKept().keys()].sort(), [2, 3], 'whole, what it no longer holds goes');
  assert.equal((await kept('covers'))?.rev, 20);
});

test('covers of another version in a whole stop, and what was held stays', async () => {
  await held();
  answers['/api/index/covers/changes'] = startAgain;
  answers['/api/index/covers'] = () => bytes(joined(head(20, 9), frame(9, picture(9))));
  await syncNow();
  assert.equal(indexSync.shows, 'stopped');
  assert.equal(coversKept().size, 2);
});

test('a feed of covers of another version asks for the whole, and only that can stop it', async () => {
  await held();
  answers['/api/index/covers/changes'] = () => bytes(head(20, 9));
  await syncNow();
  assert.ok(asked.includes('/api/index/covers'));
  assert.equal(indexSync.shows, 'idle');
});

test('covers cut short count as the rule says: a whole failed, a feed starts again, and the rev stays', async () => {
  await held();
  answers['/api/index/covers/changes'] = () => bytes(bundle(7, frame(3, picture(3))).slice(0, -5));
  answers['/api/index/covers'] = () => bytes(bundle(7, frame(3, picture(3))).slice(0, -5));
  await syncNow();
  assert.equal((await kept('covers'))?.rev, 5);
  assert.equal((await kept('covers'))?.part.failed, 1);
  assert.equal((await kept('covers'))?.part.whole, true);
});

test('a body of nothing is no covers', async () => {
  answers['/api/index/covers'] = () => bytes(new Uint8Array(0));
  await started();
  assert.equal((await kept('covers'))?.part.failed, 1);
  assert.equal((await kept('covers'))?.rev, null);
});

test('storage refusing covers is a failure, and their rev stays', async () => {
  await held();
  mock.timers.tick(1);
  storage.refused = true;
  answers['/api/index/covers/changes'] = () => bytes(bundle(7, frame(3, picture(3))));
  await syncNow();
  storage.refused = false;
  assert.equal((await kept('covers'))?.rev, 5);
});

/** What of the parts' bytes a look read. */
async function bytesRead(told: boolean) {
  const rows = rowsOf(`cratarr-index${whoOf(me)}`, 'parts');
  const get = rows.get.bind(rows);
  const read: unknown[] = [];
  rows.get = (key) => (read.push(key), get(key));
  await syncNow(told);
  rows.get = get;
  return read.filter((k) => String(k).includes('bytes'));
}

test('covers the storage will not take leave their rev where it stood, though how they stand can be written', async () => {
  await held();
  mock.timers.tick(1);
  Object.assign(storage, { refused: true, only: 'covers' });
  answers['/api/index/covers/changes'] = () => bytes(bundle(7, frame(3, picture(3))));
  await syncNow();
  Object.assign(storage, { refused: false, only: null });
  assert.equal((await kept('covers'))?.rev, 5, 'or what was not kept would never come again');
  assert.equal((await kept('covers'))?.part.failed, 1);
});

test('a whole of covers the storage will not clear for is no whole, though it brings none of its own', async () => {
  await held();
  answers['/api/index/covers/changes'] = () => new Response(JSON.stringify({ rev: 20, reset: true }), { status: 410 });
  answers['/api/index/covers'] = () => bytes(bundle(20));
  Object.assign(storage, { refused: true, only: 'covers' });
  await syncNow();
  Object.assign(storage, { refused: false, only: null });
  assert.equal((await kept('covers'))?.rev, 5);
  assert.equal(coversKept().size, 2);
});

test('a look with nothing due reads no bytes', async () => {
  await held();
  mock.timers.tick(1);
  assert.deepEqual(await bytesRead(false), []);
});

test('told, and nothing changed after all, it reads no bytes either', async () => {
  await held();
  mock.timers.tick(1);
  assert.deepEqual(await bytesRead(true), []);
});

test('a part held with its bytes gone is fetched whole', async () => {
  await held();
  rowsOf(`cratarr-index${whoOf(me)}`, 'parts').delete('index:bytes');
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  answers['/api/index'] = () => bytes(vector('after.bin'));
  await syncNow(true);
  same(await bytesKept('index'), vector('after.bin'));
  assert.equal((await kept('index'))?.part.failed, 0);
});

test('a sync keeps nothing open: a read away opens what is kept now, and the sounds stay in their bytes', async () => {
  await held();
  closeReads();
  networkSaid(false);
  assert.equal((await deviceAlbums())?.sources[0].albums.length, 3);
  reached();
  answers['/api/index/changes'] = () => bytes(vector('changes.bin'));
  await syncNow(true);
  networkSaid(false);
  assert.equal((await deviceAlbums())?.sources[0].albums.length, 4);
  reached();
});
