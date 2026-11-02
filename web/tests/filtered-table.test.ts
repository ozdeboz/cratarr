// A page's search shows its list as the table (0.13; the owner, 2026-10-31): whatever View chose, a phone's too, View's
// choice itself unchanged (lib/layoutRules); and a page's own list matched by the crate's terms where the words read
// as one, not the whole line (lib/quickCrate pageQueries).
import { test } from 'node:test';
import assert from 'node:assert/strict';
await import('./with-matcher.ts');

const { libraryShownOf, playlistShownOf, layoutFits } = await import('../src/lib/layoutRules.ts');
const { pageQueries, chipOf, leftOut } = await import('../src/lib/quickCrate.ts');
const { matches } = await import('../src/lib/search.ts');

test('searching: the table, whatever was chosen, on a phone too', () => {
  for (const chosen of ['rows', 'grid', 'table', 'flow'] as const) {
    for (const phone of [false, true]) {
      assert.equal(libraryShownOf('albums', chosen, phone, true), 'table', `${chosen} ${phone}`);
      assert.equal(libraryShownOf('composers', chosen, phone, true), 'table', `${chosen} ${phone}`);
    }
  }
  assert.equal(playlistShownOf('chips', false, true), 'table');
  assert.equal(playlistShownOf('chips', true, true), 'table');
});

test('not searching: View\'s choice where it fits, the phone without a table as before', () => {
  assert.equal(libraryShownOf('albums', 'grid', false, false), 'grid');
  assert.equal(libraryShownOf('albums', 'table', false, false), 'table');
  assert.equal(libraryShownOf('albums', 'table', true, false), 'rows');
  assert.equal(libraryShownOf('composers', 'flow', false, false), 'rows');
  assert.equal(playlistShownOf('table', true, false), 'chips');
  assert.equal(playlistShownOf('table', false, false), 'table');
  assert.equal(layoutFits('albums', 'table', true), false);
});

const genre = (v: string) => ({ kind: 'genre' as const, value: v, label: v });
const artist = (v: string, id: string) => ({ kind: 'artist' as const, value: v, label: v, id });

test('a crate\'s words: the list matched by its terms, the joining words never asked', () => {
  const line = 'hip hop and jazz from the 90s like miles davis';
  const chips = [chipOf('hip hop', [genre('Hip Hop')])!, chipOf('jazz', [genre('Jazz')])!, chipOf('miles davis', [artist('Miles Davis', 'ar-1')])!];
  const qs = pageQueries(line, chips);
  assert.deepEqual(qs, [['hip', 'hop'], ['jazz'], ['miles', 'davis']]);
  const hit = (...texts: string[]) => qs.some((q) => matches(q, ...texts));
  assert.equal(hit('Miles Davis', 'Kind of Blue'), true);
  assert.equal(matches(pageQueries(line, null)[0], 'Miles Davis', 'Kind of Blue'), false, 'the whole line finds nothing');
});

test('a left-out term is not matched; no crate: the whole line, as ever', () => {
  const chips = [chipOf('hip hop', [genre('Hip Hop')])!, leftOut(chipOf('jazz', [genre('Jazz')])!, true)];
  assert.deepEqual(pageQueries('hip hop but not jazz', chips), [['hip', 'hop']]);
  assert.deepEqual(pageQueries('miles davis', null), [['miles', 'davis']]);
  assert.deepEqual(pageQueries('', null), [[]]);
});

