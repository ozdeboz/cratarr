// What the page's list of kept tracks says, held to what Cache Storage holds (lib/offline, the real one; Finamp's A4): a browser can drop
// what it keeps, and a track listed and gone must read as not here.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
/** The streams the cache holds, and what asking it does. */
let stored: string[] = [];
let opens = () => Promise.resolve();
g.caches = { open: async () => (await opens(), { keys: async () => stored.map((p) => ({ url: `http://cratarr.test${p}` })) }) };

const { accountLearnt, perAccount } = await import('../../src/lib/account.ts');
accountLearnt('ann');
// Listed by an earlier visit, its stream dropped since. A page opened with no network never sees reads move, so this is its one look.
localStorage.setItem(perAccount('cratarr-offline-index-2'), JSON.stringify({ 'cratarr/tr-9': { source: 'cratarr', id: 'tr-9', name: 'tr-9', artist: null, album: null, album_id: 'al-1', duration_secs: 1, bytes: 1, at: 1 } }));
const { checkKept, offline, streamKey } = await import('../../src/lib/offline.svelte.ts');
const { whenReadsMove } = await import('../../src/lib/deviceIndex.svelte.ts');
for (let i = 0; i < 10; i++) await new Promise((go) => setImmediate(go));
const atOpen = Object.keys(offline.kept);

const track = (id: string) => ({ source: 'cratarr', id, name: id, artist: null, album: null, album_id: 'al-1', duration_secs: 1, bytes: 1, at: 1 });
const saved = () => Object.keys(JSON.parse(localStorage.getItem(perAccount('cratarr-offline-index-2')) ?? '{}'));

beforeEach(() => {
  offline.kept = { 'cratarr/tr-1': track('tr-1'), 'cratarr/tr-2': track('tr-2') };
  stored = [streamKey(track('tr-1')), streamKey(track('tr-2'))];
  opens = () => Promise.resolve();
});

test('a stream dropped since an earlier visit is off the list as the page opens', () => {
  assert.deepEqual(atOpen, []);
});

test('what the cache holds stays on the list', async () => {
  await checkKept();
  assert.deepEqual(Object.keys(offline.kept), ['cratarr/tr-1', 'cratarr/tr-2']);
});

test('a stream the browser dropped reads as not here, and goes from the list as saved', async () => {
  stored = [streamKey(track('tr-1'))];
  await checkKept();
  assert.deepEqual(Object.keys(offline.kept), ['cratarr/tr-1']);
  assert.deepEqual(saved(), ['cratarr/tr-1']);
});

test('a cache that cannot be read says nothing: the list stands', async () => {
  stored = [];
  opens = () => Promise.reject(new Error('no storage'));
  await checkKept();
  assert.equal(Object.keys(offline.kept).length, 2);
});

test('a stream under an id that needs encoding is found', async () => {
  const odd = track('tr/1 b');
  offline.kept = { 'cratarr/tr/1 b': odd };
  stored = [streamKey(odd)];
  await checkKept();
  assert.equal(Object.keys(offline.kept).length, 1);
});

test('a track that comes in while the cache is read is not judged by it', async () => {
  opens = async () => void (offline.kept['cratarr/tr-3'] = track('tr-3'));
  await checkKept();
  assert.ok(offline.kept['cratarr/tr-3']);
});

test('the list of an account coming is checked as it comes', async () => {
  localStorage.setItem(perAccount('cratarr-offline-index-2').replace('@ann', '@bob'), JSON.stringify({ 'cratarr/tr-1': track('tr-1'), 'cratarr/tr-5': track('tr-5') }));
  accountLearnt('bob');
  for (let i = 0; i < 10; i++) await new Promise((go) => setImmediate(go));
  assert.deepEqual(Object.keys(offline.kept), ['cratarr/tr-1']);
  accountLearnt('ann');
});

test('it is checked again as reads move, as that is when it counts', async () => {
  stored = [streamKey(track('tr-1'))];
  for (const moved of whenReadsMove) moved();
  for (let i = 0; i < 10; i++) await new Promise((go) => setImmediate(go));
  assert.deepEqual(Object.keys(offline.kept), ['cratarr/tr-1']);
});
