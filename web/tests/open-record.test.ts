// An album not in the library opened (W9, lib/openRecord): its record's page; a row no record holds made one by what its
// sources know it by (`POST /api/records/open`), never by its names; else its Bandcamp album; else nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { howToOpen } = await import('../src/lib/openRecord.ts');

const GROUP = '3cceaac3-ebe4-464d-ad70-76c74b4ae9d4';
const names = { title: 'Further', artist: 'GROUCH' };

test('a record opens by its id, whatever else the row knows', () => {
  assert.deepEqual(howToOpen({ ...names, record: 'al-88', release_group: GROUP, bandcamp: 'https://grouch.bandcamp.com/album/further' }), { record: 'al-88' });
});

test('a row no record holds is opened by its references and its names, tied to the label whose page it is on', () => {
  assert.deepEqual(howToOpen({ ...names, release_group: GROUP, year: 2011 }), { open: { release_group: GROUP, title: 'Further', artist: 'GROUCH', year: 2011 } });
  assert.deepEqual(howToOpen({ ...names, discogs_master: 12, discogs_release: 34, year: null }, 'la-5'), {
    open: { discogs_master: 12, discogs_release: 34, title: 'Further', artist: 'GROUCH', label: 'la-5' },
  });
});

test('names alone never open a record: its Bandcamp album where it has one, else nothing', () => {
  assert.deepEqual(howToOpen({ ...names, bandcamp: 'https://grouch.bandcamp.com/album/further' }), { bandcamp: 'https://grouch.bandcamp.com/album/further' });
  assert.equal(howToOpen({ ...names, year: 2011 }), null);
  assert.equal(howToOpen({ ...names, record: null, release_group: null, discogs_master: null }), null);
});
