// What a device answers for itself, through the door (lib/api, the real one, on the real index.wasm and the rules crate's own files): a
// crate, a Sounds like and a pick with Cratarr away, and what happens to each when nothing is kept to answer from.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';
import '../harness/fakeIdb.ts';

const { accountLearnt, whoOf } = await import('../../src/lib/account.ts');
const { crateRows, crateTempo, getCrate, getCrates, radioNext } = await import('../../src/lib/api.ts');
const { closeReads } = await import('../../src/lib/deviceIndex.svelte.ts');
const { heldHere } = await import('../../src/lib/deviceAnswers.svelte.ts');
const { writePart } = await import('../../src/lib/indexStore.ts');
const { networkSaid, reached } = await import('../../src/lib/reach.svelte.ts');

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const wasm = readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url));

let asked: string[] = [];
let answers = true;
globalThis.fetch = (async (input: string) => {
  const url = String(input);
  if (url.includes('index.wasm')) return new Response(wasm);
  asked.push(url);
  if (!answers) throw new TypeError('Failed to fetch');
  return new Response('[]', { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

const part = { whole: false, stopped: false, lost: 0, failed: 0, tried: null };
const keep = async (who: string, parts: [name: 'index' | 'sounds' | 'own', file: string][]) => {
  for (const [name, file] of parts) await writePart(whoOf(who), name, { rev: 5, at: 1_234_000, wants: null, part }, vector(file));
};

beforeEach(async () => {
  reached();
  answers = false;
  asked = [];
  closeReads();
  accountLearnt('ann');
  heldHere.plays = () => [];
  heldHere.tracks = () => [];
  networkSaid(false);
  await keep('ann', [['index', 'whole.bin'], ['sounds', 'sounds.bin'], ['own', 'own.bin']]);
});

test('the Crates page lists the crates kept, with their sizes as of now', async () => {
  const list = await getCrates();
  assert.deepEqual(list.map((c) => c.id), ['hearted']);
  assert.deepEqual(list[0].counts, { all: 5, only: 2, none: 3 });
  assert.equal(list[0].favourites, 'only', 'its heart, among its conditions');
  assert.equal(list[0].stats?.tracks, 2);
  assert.deepEqual(asked.filter((a) => a.startsWith('/api')), [], 'asked of nobody');
});

test('a crate opens on Best match, by the fits last heard', async () => {
  const page = await crateRows('hearted', 0, 10);
  assert.equal(page.total, 2);
  assert.deepEqual(page.rows.map((r) => r.id), ['tr-102', 'tr-101']);
  assert.equal(page.rows[0].name, 'Gust');
  const down = await crateRows('hearted', 0, 10, undefined, undefined, undefined, { order: 'own' });
  assert.deepEqual(new Set(down.rows.map((r) => r.id)), new Set(['tr-101', 'tr-102']));
});

test('what the page has not told yet counts on top of what the server last said', async () => {
  const before = (await crateRows('hearted', 0, 10)).rows.find((r) => r.id === 'tr-101');
  assert.equal(before?.play_count, 2);
  heldHere.plays = () => [{ id: 'x', source: 'cratarr', track: 'tr-101', at: 1_800_000_000_000, counts: true }];
  const after = (await crateRows('hearted', 0, 10)).rows.find((r) => r.id === 'tr-101');
  assert.equal(after?.play_count, 3);
  heldHere.plays = () => [];
  assert.equal((await crateRows('hearted', 0, 10)).rows.find((r) => r.id === 'tr-101')?.play_count, 2, 'told, the server says');
});

test('a crate says its definition and its tempo', async () => {
  const def = await getCrate('hearted');
  assert.equal(def.name, 'Hearted');
  assert.deepEqual(def.conditions.map((c) => c.axis), ['favourite']);
  assert.equal((await crateTempo('hearted')).total, 2);
});

test('a Sounds like is the true list, by the sounds kept: what is not here is on it', async () => {
  heldHere.tracks = () => ['tr-100'];
  const like = await crateRows('like:tr-100@10', 0, 20, undefined, undefined, undefined, { order: 'own' });
  assert.deepEqual(like.rows.map((r) => r.id), ['tr-100', 'tr-101', 'tr-104', 'tr-102', 'tr-103'], 'by tags alone tr-104 comes second');
});

test('a pick holds only what plays here, and says nothing of what cannot', async () => {
  heldHere.tracks = () => ['tr-100', 'tr-104'];
  for (let n = 0; n < 10; n++) {
    const { tracks } = await radioNext({ mode: 'similar', seed: ['tr-101'], exclude: ['tr-101'], last: 'tr-101', count: 5 });
    assert.deepEqual(new Set(tracks.map((t) => t.id)), new Set(['tr-100', 'tr-104']));
    assert.ok(tracks.every((t) => t.lane === 'autoplay' && t.source === 'cratarr'));
  }
  heldHere.tracks = () => [];
  assert.deepEqual((await radioNext({ mode: 'radio', seed: ['tr-101'], exclude: [], last: 'tr-101', count: 5 })).tracks, []);
});

test('with nothing of the account\'s kept, a list of crates goes where any read goes', async () => {
  closeReads();
  accountLearnt('bob');
  await keep('bob', [['index', 'whole.bin']]);
  await assert.rejects(getCrates());
  assert.ok(asked.some((a) => a.startsWith('/api/crates')), 'to Cratarr, and its last copy');
});

test('with nothing kept at all a read goes where any read goes', async () => {
  closeReads();
  accountLearnt('carol');
  await assert.rejects(crateRows('hearted', 0, 10));
  await assert.rejects(radioNext({ mode: 'similar', seed: [], exclude: [], last: null, count: 5 }));
});

test('Cratarr answering, none of this is opened', async () => {
  answers = true;
  reached();
  networkSaid(true);
  await getCrates();
  assert.deepEqual(asked, ['/api/crates']);
});
