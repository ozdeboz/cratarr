// MORE (the owner, on production 0.17.0): a record of several artist chips showed "More from" for its first artist alone. Now a row
// per chip, in the chips' order, each only where that artist has other records here, and a record shown under an earlier chip not
// shown again. The rule runs (lib/albumMore); the record's foot, which takes a whole app, is read. Red first: lib/albumMore did not
// exist at 7da786c91, and the foot named one artist.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { moreArtists, moreFrom } from '../src/lib/albumMore.ts';

const src = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const rec = (id: string, artist: string, year: number, artists?: string[]) => ({ id, title: id, artist, year, artists });

// The record shown, by both; another by each; one more by both; and one by neither.
const SHOWN = rec('al-1', 'Nicola Conte & Gianluca Petrella', 2008, ['Nicola Conte', 'Gianluca Petrella']);
const LIBRARY = [
  SHOWN,
  rec('al-2', 'Nicola Conte', 2004),
  rec('al-3', 'Gianluca Petrella', 2006),
  rec('al-4', 'Nicola Conte & Gianluca Petrella', 2011, ['Nicola Conte', 'Gianluca Petrella']),
  rec('al-5', 'Bonobo', 2010),
];
const CHIPS = [
  { ask: 'ar-1', label: 'Nicola Conte' },
  { ask: 'ar-2', label: 'Gianluca Petrella' },
];

/** Each row under the record, as the foot draws them: artist and record ids, the empty ones left out. */
const rows = (chips: typeof CHIPS, main: string | null) => {
  const by = moreArtists(chips, main);
  return by.map((artist, i) => ({ artist, ids: moreFrom(LIBRARY, SHOWN.id, artist, by.slice(0, i)).map((a) => a.id) })).filter((r) => r.ids.length);
};

test('two chips, each with another record here: both rows show, in the chips\' order; the record by both once, under the first', () => {
  assert.deepEqual(rows(CHIPS, 'Nicola Conte'), [
    { artist: 'Nicola Conte', ids: ['al-4', 'al-2'] },
    { artist: 'Gianluca Petrella', ids: ['al-3'] },
  ]);
});

test('an artist with nothing else here shows no row', () => {
  const chips = [...CHIPS, { ask: 'ar-9', label: 'Mario Biondi' }];
  assert.deepEqual(rows(chips, 'Nicola Conte').map((r) => r.artist), ['Nicola Conte', 'Gianluca Petrella']);
  assert.deepEqual(moreFrom(LIBRARY, SHOWN.id, 'Mario Biondi'), []);
  // Nor one whose records are all under a row above.
  const shared = [rec('al-1', 'A & B', 2000, ['A', 'B']), rec('al-7', 'A & B', 2001, ['A', 'B'])];
  assert.deepEqual(moreFrom(shared, 'al-1', 'B', ['A']), []);
});

test("a record of one chip keeps its main artist's row, as before; the record itself never shows under it", () => {
  assert.deepEqual(moreArtists([{ ask: 'ar-1', label: 'Nicola Conte feat. Bebo Ferra' }], 'Nicola Conte'), ['Nicola Conte']);
  assert.deepEqual(moreArtists([], null), []);
  assert.ok(!moreFrom(LIBRARY, SHOWN.id, 'Nicola Conte').some((a) => a.id === SHOWN.id));
  // A chip named twice is one row.
  assert.deepEqual(moreArtists([...CHIPS, { ask: null, label: 'nicola conte' }], null), ['Nicola Conte', 'Gianluca Petrella']);
});

test("the record's foot draws one AlbumMore per chip, each told the rows above; AlbumMore leaves their records out", () => {
  const view = src('PlaylistView.svelte');
  assert.ok(view.includes('{@const by = moreArtists(albumChipsShown, listedHere?.a.artists?.[0] ?? mainArtist(albumHead.artist))}'));
  assert.ok(view.includes('{#each by as artist, i (artist)}<AlbumMore source={album.source} album={album.id} {artist} above={by.slice(0, i)} />{/each}'));
  const more = src('AlbumMore.svelte');
  assert.ok(more.includes("let { source, album, artist, above = [] }: { source: string; album: string; artist: string; above?: readonly string[] } = $props();"));
  assert.ok(more.includes('const others = $derived(moreFrom(library.view?.sources.find((x) => x.id === source)?.albums ?? [], album, artist, above));'));
  assert.ok(more.includes('{#if others.length}'), 'no row where there is nothing');
});
