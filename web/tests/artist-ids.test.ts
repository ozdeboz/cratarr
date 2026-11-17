// One artist page by id (W10, lib/artistIds): Cratarr's own artist asked for by their id for life, opened by name where only a
// name is known and moved to the id the server answers; Bandcamp's and Discover's artists go to Cratarr's page, a server's and
// the device's stay by name; a number only where a list shows two of one name; a related or Beyond artist opened by id, else by
// their MusicBrainz id, else by name.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { artistAsk, artistLabel, artistOpen, artistSource, artistTitle, isArtistId, movedTo } = await import('../src/lib/artistIds.ts');

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
