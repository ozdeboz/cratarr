// What a read answers from the device (lib/deviceReads): the rules a list holds that the small library of the both-ways test does not
// reach. That test, tests/isolated/reads-both-ways, reads it field by field against Cratarr's answers.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { listView, recordNumber, recordView } = await import('../src/lib/deviceReads.ts');
type LibraryRow = import('../src/lib/deviceReads.ts').LibraryRow;

const library = (id: number, order: number, kind: LibraryRow['kind'], name: string): LibraryRow => ({ id, key: `k${id}`, name, kind, order, colour: null, owner: null });
const record = {
  id: 'al-1',
  title: 'T',
  sort: 't',
  artist: '',
  artists: [],
  year: null,
  original_year: null,
  added: null,
  kind: null,
  secondary: [],
  genres: [],
  whole_tracks: null,
  tracks: 0,
  seconds: null,
  copies: [],
};

test('libraries lie in their own order, whatever their ids', () => {
  const view = listView({ rev: 1, libraries: [library(1, 2, 'incoming', 'In'), library(2, 0, 'own', 'Own'), library(3, 1, 'main', 'Main')], records: [] }, null, 0);
  assert.deepEqual(view.sources[0].libraries, ['k2', 'k3', 'k1']);
  assert.deepEqual(view.sources[0].library_info.map((l) => l.id), ['k2', 'k3', 'k1']);
});

test('Music\'s name is the first main library\'s, not the first library\'s', () => {
  assert.equal(listView({ rev: 1, libraries: [library(2, 0, 'own', 'Own'), library(3, 1, 'main', 'Main')], records: [] }, null, 0).music, 'Main');
  assert.equal(listView({ rev: 1, libraries: [library(2, 0, 'own', 'Own')], records: [] }, null, 0).music, null);
});

test('a record with no artist says none, and one with no copy has no format, no path and none to play', () => {
  const [a] = listView({ rev: 1, libraries: [], records: [record] }, null, 0).sources[0].albums;
  assert.equal(a.artist, null);
  assert.equal(a.path, null);
  assert.equal(a.format, null);
  assert.deepEqual(a.copies, []);
  const view = recordView({ record, tracks: [] });
  assert.equal(view.artist, null);
  assert.equal(view.format, null);
  assert.equal('added' in view, false);
});

test('a library\'s key ending in a slash makes no double slash in a path', () => {
  const copy = { library: 'k/', dir: 'd', format: null, kbps: null, bits: null, rate: null, size: null, added: null, tracks: 1 };
  assert.equal(listView({ rev: 1, libraries: [], records: [{ ...record, copies: [copy] }] }, null, 0).sources[0].albums[0].path, 'k/d');
});

test('nothing a viewer may do is offered from the device: those are Cratarr\'s to say', () => {
  const copy = { library: 'k', dir: 'd', format: null, kbps: null, bits: null, rate: null, size: null, added: null, tracks: 1 };
  const [a] = listView({ rev: 1, libraries: [], records: [{ ...record, copies: [copy] }] }, null, 0).sources[0].albums;
  assert.deepEqual([a.copies?.[0].deletable, a.copies?.[0].keepable, a.copies?.[0].keep_to], [false, false, null]);
});

test('a row\'s file is its line\'s last part', () => {
  const view = recordView({ record, tracks: [{ id: 'tr-1', disc: 1, number: 1, title: 'x', artist: null, line: 'a/b/c.flac', seconds: 1, year: null, bpm: null, born: null }] });
  assert.equal(view.rows[0].file, 'c.flac');
  assert.equal(view.rows[0].entry, 'a/b/c.flac');
});

test('only a record\'s own kind of id is one', () => {
  assert.equal(recordNumber('al-12'), 12);
  assert.equal(recordNumber('tr-12'), null);
  assert.equal(recordNumber('al-'), null);
  assert.equal(recordNumber('al-1x'), null);
  assert.equal(recordNumber('old-hash'), null);
});
