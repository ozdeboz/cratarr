// One artist page by id (W10, lib/artistIds): Cratarr's own artist asked for by their id for life, opened by name where only a
// name is known and moved to the id the server answers; Bandcamp's and Discover's artists go to Cratarr's page, a server's and
// the device's stay by name; a number only where a list shows two of one name; a related or Beyond artist opened by id, else by
// their MusicBrainz id, else by name.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { albumChips, artistAsk, artistLabel, artistOpen, artistSource, artistTitle, isArtistId, movedTo } = await import('../src/lib/artistIds.ts');

test('an id is asked by id, a name by name, and an artist called like an id is no id', () => {
  assert.deepEqual(artistAsk('ar-12'), { id: 'ar-12' });
  assert.deepEqual(artistAsk('Rone'), { name: 'Rone' });
  assert.deepEqual(artistAsk('ar-'), { name: 'ar-' });
  assert.deepEqual(artistAsk('ar-12 Remixed'), { name: 'ar-12 Remixed' });
  assert.ok(isArtistId('ar-1') && !isArtistId('al-1') && !isArtistId('AR-1'));
});

test('opened by name, the page moves to the id the server answers; opened by id, or of a name nobody has, it stays', () => {
  assert.equal(movedTo('cratarr', 'Rone', { id: 'ar-31' }), 'ar-31');
  assert.equal(movedTo('cratarr', 'ar-31', { id: 'ar-31' }), null);
  assert.equal(movedTo('cratarr', 'Nobody', { id: null }), null);
  assert.equal(movedTo('cratarr', 'Nobody', {}), null);
});

test("Bandcamp's and Discover's artists go to Cratarr's page; a server's and the device's stay by name", () => {
  assert.equal(artistSource('bandcamp'), 'cratarr');
  assert.equal(artistSource('discover'), 'cratarr');
  assert.equal(artistSource('cratarr'), 'cratarr');
  assert.equal(artistSource('device'), 'device');
  assert.equal(artistSource('jellyfin-home'), 'jellyfin-home');
  assert.equal(movedTo('device', 'Rone', { id: 'ar-31' }), null);
  assert.equal(movedTo('jellyfin-home', 'Rone', { id: 'ar-31' }), null);
});

test("the page's title is the name the server says, plain with namesakes; an id says nothing until then", () => {
  assert.equal(artistTitle('ar-31', 'Rone'), 'Rone');
  assert.equal(artistTitle('ar-31', undefined), '');
  assert.equal(artistTitle('Rone', null), 'Rone');
  assert.equal(artistTitle('ar-31', 'ar-31'), '');
});

test('a number only where a list shows two of one name, and the server gave it', () => {
  const shown = ['Rone', 'rone ', 'Air'];
  assert.equal(artistLabel('Rone', 2, shown), 'Rone (2)');
  assert.equal(artistLabel('Air', 1, shown), 'Air');
  assert.equal(artistLabel('Rone', undefined, shown), 'Rone');
  assert.equal(artistLabel('Rone', 1, ['Rone']), 'Rone');
});

test('a related or Beyond artist opens by their id, else by their MusicBrainz id, else by name', () => {
  assert.deepEqual(artistOpen({ id: 'ar-4', mbid: 'm1', name: 'Rone' }), { id: 'ar-4' });
  assert.deepEqual(artistOpen({ id: null, mbid: 'm1', name: 'Rone' }), { mbid: 'm1', name: 'Rone' });
  assert.deepEqual(artistOpen({ name: 'Rone' }), { name: 'Rone' });
  assert.deepEqual(artistOpen({ id: 'Rone', name: 'Rone' }), { name: 'Rone' }, 'a name is no id');
});

test("a record's chips: one per artist of its own credit, with ids, before the library list has it (M8); each asks by id (M9)", () => {
  const two = [
    { name: 'Glass Harbour', id: 'ar-4' },
    { name: 'Kite Theory', id: 'ar-9' },
  ];
  // A record not had has no row in the library list: its view's credit alone gives the chips.
  assert.deepEqual(albumChips('Glass Harbour & Kite Theory', 'Glass Harbour & Kite Theory', two, null), [
    { ask: 'ar-4', label: 'Glass Harbour' },
    { ask: 'ar-9', label: 'Kite Theory' },
  ]);
  // A record had, opened before the list loads, and after: two chips both times, never one then two.
  assert.deepEqual(albumChips('Glass Harbour & Kite Theory', 'Glass Harbour & Kite Theory', two, ['Glass Harbour', 'Kite Theory']), albumChips('Glass Harbour & Kite Theory', 'Glass Harbour & Kite Theory', two, null));
  // The second of two of one name: their photo, their heart and their page, by their id, never the first's by the name.
  assert.deepEqual(albumChips('Coma', 'Coma', [{ name: 'Coma', id: 'ar-77' }], ['Coma']), [{ ask: 'ar-77', label: 'Coma' }]);
  // One credited: the credit's whole line on the chip, as before.
  assert.deepEqual(albumChips('Glass Harbour feat. Ana', 'Glass Harbour', [{ name: 'Glass Harbour', id: 'ar-4' }], null), [{ ask: 'ar-4', label: 'Glass Harbour feat. Ana' }]);
});

test("without the view's credit (another server, the device, an answer before it): the list's names, else the main artist, by name", () => {
  assert.deepEqual(albumChips('A & B', 'A & B', undefined, ['A', 'B']), [
    { ask: 'A', label: 'A' },
    { ask: 'B', label: 'B' },
  ]);
  assert.deepEqual(albumChips('A feat. B', 'A', undefined, null), [{ ask: 'A', label: 'A feat. B' }]);
  assert.deepEqual(albumChips('A feat. B', 'A', [], []), [{ ask: 'A', label: 'A feat. B' }]);
  // A credit with no id is asked by its name; an id of another kind is no artist's.
  assert.deepEqual(albumChips('A & B', 'A & B', [{ name: 'A', id: null }, { name: 'B', id: 'al-3' }], null), [
    { ask: 'A', label: 'A' },
    { ask: 'B', label: 'B' },
  ]);
  assert.deepEqual(albumChips(null, null, undefined, null), []);
  assert.deepEqual(albumChips('feat. X', null, undefined, null), [{ ask: null, label: 'feat. X' }], 'no one to open: the chip shown, unable');
});

test("the record page's chips are these, over its view's credit; each asks for its photo and heart, and opens, by its own ask", () => {
  const src = readFileSync(new URL('../src/lib/PlaylistView.svelte', import.meta.url), 'utf8');
  assert.match(src, /album && albumHead \? albumChips\(albumHead\.artist, mainArtist\(albumHead\.artist\), albumHead\.artists, listedHere\?\.a\.artists\) : \[\]/);
  assert.match(src, /labels: r\.labels, artists: r\.artists, was_until/, "the view's credit kept with its head");
  assert.match(src, /artistFavorite\(from, ask\)[\s\S]{0,200}getArtistAbout\(ask\)/);
  assert.match(src, /onclick=\{\(\) => chip\.ask && goToArtist\(album\.source, chip\.ask\)\}/);
});
