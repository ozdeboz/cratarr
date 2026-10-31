// Navigation as one step (lib/navStep): what a tap leaves goes when where it goes shows, never before, and not at all
// when it never shows.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { arm, merge, step, MOVE_MS } = await import('../src/lib/navStep.ts');
const lib = { page: 'library', viewed: null };
const album = { page: 'library', viewed: 'album:x' };

test('let go once where it goes shows, not at the tap', () => {
  let p = arm(null, { search: 'clear' }, lib, 0);
  let r = step(p, lib, lib, 10);
  assert.equal(r.apply, null, 'nothing moved yet');
  r = step(r.pending!, album, lib, 20);
  assert.equal(r.apply, null, 'the panel asked, not shown yet');
  r = step(r.pending!, album, album, 300);
  assert.deepEqual(r.apply, { search: 'clear' });
  assert.equal(r.pending, null);
});

test('back before it showed: nothing lost', () => {
  const p = arm(null, { search: 'clear', player: true }, lib, 0);
  let r = step(p, album, lib, 10);
  r = step(r.pending!, lib, lib, 20);
  assert.deepEqual(r, { pending: null, apply: null });
});

test('nothing moved (a menu opened instead): dropped after a while, nothing let go', () => {
  const p = arm(null, { search: 'clear' }, lib, 0);
  assert.notEqual(step(p, lib, lib, MOVE_MS - 1).pending, null);
  assert.deepEqual(step(p, lib, lib, MOVE_MS + 1), { pending: null, apply: null });
});

test('a page changed: let go when the new page shows', () => {
  const home = { page: 'home', viewed: null };
  const p = arm(null, { search: 'clear' }, home, 0, true);
  assert.equal(step(p, home, lib, 5).apply, null, 'the Library still shows');
  assert.deepEqual(step(p, home, home, 50).apply, { search: 'clear' });
});

test('back to a search: its words come back, over emptying them', () => {
  const back = { q: 'miles', scope: 'everywhere' as const };
  assert.deepEqual(merge({ search: 'clear' }, { restore: back }), { restore: back });
  assert.deepEqual(merge({ restore: back }, { search: 'clear' }), { restore: back });
  assert.deepEqual(merge({ search: 'page' }, { search: 'clear', player: true }), { search: 'clear', player: true });
  // A crate saved from the words being made: they go when it shows, whatever else the step leaves.
  assert.deepEqual(merge({ search: 'clear' }, { quick: true }), { search: 'clear', quick: true });
  const p = arm(arm(null, { restore: back }, lib, 0, true), { search: 'clear' }, lib, 1, true);
  assert.deepEqual(step(p, lib, lib, 2).apply, { restore: back });
});

test("a search's answers kept by their words, the newest few", async () => {
  const { keep } = await import('../src/lib/searchKept.ts');
  const m = new Map<string, number>();
  for (let i = 0; i < 5; i++) keep(m, `q${i}`, i, 3);
  assert.deepEqual([...m.keys()], ['q2', 'q3', 'q4']);
  keep(m, 'q2', 9, 3);
  assert.deepEqual([...m.keys()], ['q3', 'q4', 'q2'], 'asked again: the newest');
});

test('a breadcrumb: the panel closed, then the page changed; let go only on the page it goes to', () => {
  const p = arm(null, { layer: true }, { page: 'insights', viewed: 'insights:map', hash: '#/insights' }, 0, false, '#/settings');
  const closed = { page: 'insights', viewed: null, hash: '#/insights' };
  let r = step(p, closed, closed, 10);
  assert.equal(r.apply, null, 'the panel closed, the page not changed yet');
  const settings = { page: 'settings', viewed: null, hash: '#/settings' };
  r = step(r.pending!, settings, closed, 20);
  assert.equal(r.apply, null, 'Settings asked, the page before still shows');
  assert.deepEqual(step(r.pending!, settings, settings, 300).apply, { layer: true });
});
