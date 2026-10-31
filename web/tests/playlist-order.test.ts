// The playlists' orders (lib/playlistOrder; the owner, 2026-10-31): Playing first off the page, kept in a menu as
// Ticked first; Length and Last played by the playlists' facts, one without the fact last whichever way round; the
// table's columns that order it.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { LIST_ORDERS, PLAYLIST_ORDERS, PLAYLIST_ORDER_OF, playlistBy } = await import('../src/lib/playlistOrder.ts');

type P = { id: string; name: string; count: number; dead?: number };
const p = (id: string, count = 1, dead = 0): P => ({ id, name: id, count, dead });
const facts: Record<string, { seconds: number; last_played: number | null }> = {
  a: { seconds: 600, last_played: 30 },
  b: { seconds: 1800, last_played: null },
  c: { seconds: 0, last_played: 10 },
  // d: no facts at all
};
const lists = [p('d'), p('c'), p('b'), p('a')];
const order = (o: string, down: boolean) =>
  [...lists].sort(playlistBy<P>(o as never, down, () => false, (x) => facts[x.id])).map((x) => x.id);

test('the page has no Playing first; a menu keeps it as Ticked first', () => {
  assert.deepEqual(PLAYLIST_ORDERS.map((o) => o.label), ['Name', 'Tracks', 'Length', 'Last played', 'Missing']);
  assert.deepEqual(LIST_ORDERS.map((o) => o.label), ['Name', 'Tracks', 'Ticked first']);
});

test('Length and Last played start most first (the head\'s first tap turns them round)', () => {
  for (const v of ['length', 'lastPlayed']) assert.equal(PLAYLIST_ORDERS.find((o) => o.value === v)?.most, true);
});

test('by length: shortest or longest first, those without one (none read, or empty) last both ways', () => {
  assert.deepEqual(order('length', false), ['a', 'b', 'c', 'd']);
  assert.deepEqual(order('length', true), ['b', 'a', 'd', 'c']);
});

test('by last played: never played and not read last both ways', () => {
  assert.deepEqual(order('lastPlayed', false), ['c', 'a', 'b', 'd']);
  assert.deepEqual(order('lastPlayed', true), ['a', 'c', 'd', 'b']);
});

test('the others as before, turned round with the arrow', () => {
  const ls = [p('x', 3, 0), p('y', 1, 2), p('z', 2, 1)];
  const by = (o: string, down: boolean, holds: (q: P) => boolean = () => false) => [...ls].sort(playlistBy<P>(o as never, down, holds)).map((q) => q.id);
  assert.deepEqual(by('tracks', false), ['y', 'z', 'x']);
  assert.deepEqual(by('tracks', true), ['x', 'z', 'y']);
  assert.deepEqual(by('missing', true), ['y', 'z', 'x']);
  assert.deepEqual(by('name', true), ['z', 'y', 'x']);
  assert.deepEqual(by('playing', false, (q) => q.id === 'z'), ['z', 'x', 'y']);
});

test('the table orders by Length, Last played and Missing; not by Genres, Changed or Unplayed', () => {
  assert.deepEqual(Object.keys(PLAYLIST_ORDER_OF).sort(), ['lastPlayed', 'length', 'missing']);
  for (const id of ['genres', 'changed', 'unplayed']) assert.equal(PLAYLIST_ORDER_OF[id], undefined);
});
