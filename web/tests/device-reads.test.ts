// What a read answers from the device (lib/deviceReads): the rules a list holds that the small library of the both-ways test does not
// reach. That test, tests/isolated/reads-both-ways, reads it field by field against Cratarr's answers.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { artistPanel, foundView, listView, recordNumber, recordView } = await import('../src/lib/deviceReads.ts');
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

const track = { id: 'tr-1', disc: 1, number: 1, title: 'So What', artist: 'Miles Davis', line: 'a/b/1.flac', seconds: 60, year: 1959, bpm: null, born: null, record: 'al-3', album: 'Kind of Blue' };

test('an artist found reads as Cratarr lists one, one album said as one', () => {
  const found = foundView({ artists: { total: 2, rows: [{ id: 'ar-7', name: 'Miles Davis', records: 1, score: 5 }, { id: 'ar-8', name: 'Mo', records: 3, score: 4 }] } }, 'm', 0);
  assert.equal(found.q, 'm');
  assert.equal(found.groups.artist!.total, 2);
  assert.deepEqual(found.groups.artist!.items[0], {
    kind: 'artist',
    subject: 'artist:cratarr/Miles Davis',
    title: 'Miles Davis',
    sub: '1 album',
    cover: '/api/artists/ar-7/photo',
    id: 'ar-7',
    score: 5,
    matched: [{ on: 'name', text: 'Miles Davis' }],
  });
  assert.equal(found.groups.artist!.items[1].sub, '3 albums');
});

test('an artist found with tracks and no album of their own says no count, never "0 albums"', () => {
  // The owner, testing 0.16.0: "C Cat Trance" found by a word said "0 albums".
  const found = foundView({ artists: { total: 1, rows: [{ id: 'ar-9', name: 'C Cat Trance', records: 0, score: 5 }] } }, 'trance', 0);
  assert.equal(found.groups.artist!.items[0].sub, '');
});

test('a record found says its artist and year, and an artist of none or a year of none is left out of the line', () => {
  const found = foundView({ records: { total: 2, rows: [{ id: 'al-3', title: 'T', artist: 'A', year: 1959, score: 5, on: 'name' }, { id: 'al-4', title: 'U', artist: '', year: null, score: 4, on: 'name' }] } }, 't', 0);
  assert.deepEqual(found.groups.album!.items[0], { kind: 'album', subject: 'album:cratarr/al-3', title: 'T', sub: 'A · 1959', cover: '/api/albums/cratarr/al-3/cover', score: 5, matched: [{ on: 'name', text: 'T' }] });
  assert.equal(found.groups.album!.items[1].sub, '');
});

test('what was found through its artist says so', () => {
  const found = foundView({ records: { total: 1, rows: [{ id: 'al-3', title: 'T', artist: 'A', year: null, score: 0.05, on: 'artist' }] } }, 'a', 0);
  assert.deepEqual(found.groups.album!.items[0].matched, [{ on: 'artist', text: 'A' }]);
});

test('a track found opens its record, and says what its menu needs', () => {
  const found = foundView({ tracks: { total: 1, rows: [{ ...track, score: 5, on: 'name' }] } }, 'so', 0);
  assert.deepEqual(found.groups.track!.items[0], {
    kind: 'track',
    subject: 'album:cratarr/al-3',
    title: 'So What',
    sub: 'Miles Davis · Kind of Blue',
    cover: '/api/albums/cratarr/al-3/cover',
    track: 'tr-1',
    artist: 'Miles Davis',
    album: 'Kind of Blue',
    album_id: 'al-3',
    duration_secs: 60,
    score: 5,
    matched: [{ on: 'name', text: 'So What' }],
  });
});

test('the kinds not asked are absent, not empty', () => {
  assert.deepEqual(Object.keys(foundView({ tracks: { total: 0, rows: [] } }, 'x', 0).groups), ['track']);
});

test("top is every kind's best by its weight, as Cratarr's", () => {
  const found = foundView(
    {
      artists: { total: 1, rows: [{ id: 'ar-1', name: 'Artist', records: 1, score: 5 }] },
      records: { total: 1, rows: [{ id: 'al-1', title: 'Record', artist: 'A', year: null, score: 5.2, on: 'name' }] },
      tracks: { total: 1, rows: [{ ...track, title: 'Track', score: 6, on: 'name' }] },
    },
    'x',
    0,
  );
  assert.deepEqual(found.top.map((i) => i.title), ['Artist', 'Record', 'Track']);
  assert.equal(found.top[2].score, 6 * 0.8);
  assert.equal(found.groups.track!.items[0].score, 6, 'its group keeps its own score');
});

test('top holds eight at most, and none past the first page', () => {
  const rows = Array.from({ length: 12 }, (_, n) => ({ id: `ar-${n}`, name: `A${n}`, records: 1, score: 12 - n }));
  const records = Array.from({ length: 12 }, (_, n) => ({ id: `al-${n}`, title: `R${n}`, artist: 'A', year: null, score: 12 - n, on: 'name' as const }));
  assert.equal(foundView({ artists: { total: 12, rows }, records: { total: 12, rows: records } }, 'a', 0).top.length, 8);
  assert.deepEqual(foundView({ artists: { total: 12, rows } }, 'a', 5).top, []);
});

test("an artist's panel reads their records and their tracks as Cratarr's, each track with its record", () => {
  const panel = artistPanel({ albums: [{ ...record, id: 'al-3', title: 'Own' }], appears_on: [{ ...record, id: 'al-4', title: 'Guest' }], tracks: [track] }, 'Miles Davis');
  assert.equal(panel.source, 'cratarr');
  assert.equal(panel.name, 'Miles Davis');
  assert.deepEqual(panel.albums.map((a) => a.title), ['Own']);
  assert.deepEqual(panel.appears_on.map((a) => a.title), ['Guest']);
  assert.equal(panel.rows[0].id, 'tr-1');
  assert.equal(panel.rows[0].album, 'Kind of Blue');
  assert.equal(panel.rows[0].album_id, 'al-3');
  assert.equal(panel.rows[0].name, 'So What');
});
