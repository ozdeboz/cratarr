// Available offline, the Library's switch (lib/listFilter, the real one): what this device holds is no word of Cratarr's, so it is
// answered here, with Cratarr away too, and goes along with whatever Cratarr answers of the rest.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { effects } from '../harness/pageGlobals.ts';

const { accountLearnt, perAccount } = await import('../../src/lib/account.ts');
accountLearnt('ann');
const kept = (id: string, album: string) => ({ source: 'cratarr', id, name: id, artist: null, album: null, album_id: album, duration_secs: 1, bytes: 1, at: 1 });
localStorage.setItem(perAccount('cratarr-offline-index-2'), JSON.stringify({ 'cratarr/tr-1': kept('tr-1', 'al-1'), 'cratarr/tr-2': kept('tr-2', 'al-2') }));

/** What the filter was asked, and what Cratarr answers. */
let asked: { switches?: string[]; explain?: boolean }[] = [];
let answer: () => Response = () => new Response(JSON.stringify({ keys: ['al-1', 'al-3'] }), { headers: { 'content-type': 'application/json' } });
globalThis.fetch = (async (_: string, init?: RequestInit) => {
  asked.push(JSON.parse(String(init?.body ?? '{}')));
  return answer();
}) as typeof fetch;

const { askAtOnce, filterBlocked, filtersOn, passing, switchesOn } = await import('../../src/lib/listFilter.svelte.ts');
const { HERE, SWITCHES } = await import('../../src/lib/pageView.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');

const library = { list: { kind: 'library' as const } };
const on = (...keys: string[]) => (prefs.filters.library = { conditions: [], on: keys });

/** The Library's filter, run once: what passes, once any answer is in. */
async function shown(at: typeof library | { list: { kind: 'crate'; id: string } } = library) {
  const out = passing(() => 'library', () => at as never);
  const before = effects.length;
  askAtOnce('library');
  effects[before - 1]();
  await new Promise((go) => setTimeout(go, 10));
  return out;
}

beforeEach(() => {
  asked = [];
  answer = () => new Response(JSON.stringify({ keys: ['al-1', 'al-3'] }), { headers: { 'content-type': 'application/json' } });
  on();
});

test('the Library offers it by the words of decision 0085', () => {
  assert.ok(SWITCHES.library.some((s) => s.key === HERE && s.label === 'Available offline'));
});

test('on by itself it needs nobody: what is here passes, and nothing is asked', async () => {
  on(HERE);
  const out = await shown();
  assert.deepEqual([...out.keys!].sort(), ['al-1', 'al-2']);
  assert.equal(out.pending, false);
  assert.deepEqual(asked, []);
});

test('beside what Cratarr answers, a record must pass both, and Cratarr is not asked what it cannot know', async () => {
  on(HERE, 'favourites');
  const out = await shown();
  assert.deepEqual([...out.keys!], ['al-1']);
  assert.deepEqual(asked[0].switches, ['favourites']);
});

test('off, what Cratarr answers stands', async () => {
  on('favourites');
  const out = await shown();
  assert.deepEqual([...out.keys!].sort(), ['al-1', 'al-3']);
});

test('nothing on is no filter', async () => {
  const out = await shown();
  assert.equal(out.keys, null);
  assert.deepEqual(asked, []);
});

test('a crate\'s rows come filtered, so it filters none of them', async () => {
  on(HERE);
  const out = await shown({ list: { kind: 'crate', id: 'c' } });
  assert.equal(out.keys, null);
});

test('it counts as a filter on, and shows as one', () => {
  on(HERE);
  assert.deepEqual(switchesOn('library'), [HERE]);
  assert.equal(filtersOn('library'), 1);
});

test('what keeps a record out is asked of Cratarr without it', async () => {
  answer = () => new Response(JSON.stringify({ keys: [], blocked: {} }), { headers: { 'content-type': 'application/json' } });
  on(HERE, 'favourites');
  await filterBlocked('library', { kind: 'library' });
  assert.deepEqual(asked[0].switches, ['favourites']);
});
