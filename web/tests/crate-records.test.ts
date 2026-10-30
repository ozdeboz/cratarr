// A crate as records (lib/crateRecords; the owner, 2026-10-30): its tracks grouped by album in the crate's order, the
// album orders, the crates that start as records, and what a record holds of its album.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { crateViewOf, holdsLine, orderRecords, recordCell, recordsOf } = await import('../src/lib/crateRecords.ts');

const row = (album_id: string | undefined, album: string, artist: string, year?: number, added?: number) => ({ album_id, album, artist, year, added });
const rows = [
  row('b', 'Blue', 'Miles', 1959, 10),
  row('a', 'Alpha', 'Zed', 2001, 30),
  row('b', 'Blue', 'Miles', 1959, 12),
  row(undefined, 'Loose One', 'Ann'),
  row(undefined, 'loose one', 'ANN'),
].map((r, i) => ({ r, i }));

test('grouped by album in the order the first of each comes, a row without an id by its name and artist', () => {
  const recs = recordsOf(rows);
  assert.deepEqual(recs.map((r) => [r.title, r.at]), [['Blue', [0, 2]], ['Alpha', [1]], ['Loose One', [3, 4]]]);
  assert.equal(recs[0].added, 12, 'a record was added when its newest track was');
});

test('the album orders, equals in the crate order; the crate order turned round', () => {
  const recs = recordsOf(rows);
  assert.deepEqual(orderRecords(recs, 'own', false).map((r) => r.title), ['Blue', 'Alpha', 'Loose One']);
  assert.deepEqual(orderRecords(recs, 'own', true).map((r) => r.title), ['Loose One', 'Alpha', 'Blue']);
  assert.deepEqual(orderRecords(recs, 'artist', false).map((r) => r.artist), ['Ann', 'Miles', 'Zed']);
  assert.deepEqual(orderRecords(recs, 'year', false).map((r) => r.title), ['Loose One', 'Blue', 'Alpha'], 'no year first');
  assert.deepEqual(orderRecords(recs, 'added', true).map((r) => r.title), ['Alpha', 'Blue', 'Loose One']);
});

test("Home's album crates start as records, the rest as tracks, a choice kept per crate", () => {
  assert.equal(crateViewOf('home:new', {}), 'records');
  assert.equal(crateViewOf('home:graves', {}), 'records');
  assert.equal(crateViewOf('like:week', {}), 'tracks');
  assert.equal(crateViewOf('c1', {}), 'tracks');
  assert.equal(crateViewOf('home:new', { 'home:new': 'tracks' }), 'tracks');
  assert.equal(crateViewOf('c1', { c1: 'records' }), 'records');
});

test('what a record holds of its album', () => {
  assert.equal(holdsLine(3, 10), '3 of 10 tracks');
  assert.equal(holdsLine(1, 1), '1 of 1 track');
  assert.equal(holdsLine(3, null), '3 tracks');
  assert.equal(holdsLine(1, undefined), '1 track');
  assert.equal(holdsLine(4, 2), '4 tracks', "an album's count below the crate's: not trusted");
});

test("a record's album total as its rows say it (album_tracks), else not said", () => {
  const recs = recordsOf([{ r: { album_id: 'x', album: 'X', artist: 'Y', album_tracks: 10 }, i: 0 }, { r: { album_id: 'z', album: 'Z', artist: 'Y' }, i: 1 }]);
  assert.equal(recs[0].of, 10);
  assert.equal(recs[1].of, null);
  assert.equal(holdsLine(3, recs[0].of), '3 of 10 tracks');
});

test('as a table: ordered by how many of its tracks the crate holds and by their length; its cells', () => {
  const timed = [
    { r: { ...row('b', 'Blue', 'Miles', 1959, 10), duration_secs: 300 }, i: 0 },
    { r: { ...row('a', 'Alpha', 'Zed', 2001, 30), duration_secs: 900 }, i: 1 },
    { r: { ...row('b', 'Blue', 'Miles', 1959, 12), duration_secs: 200 }, i: 2 },
  ];
  const recs = recordsOf(timed);
  assert.deepEqual(orderRecords(recs, 'tracks', true).map((r) => r.title), ['Blue', 'Alpha'], 'most tracks first');
  assert.deepEqual(orderRecords(recs, 'length', true).map((r) => r.title), ['Alpha', 'Blue'], 'longest first');
  const blue = recs[0];
  assert.equal(blue.secs, 500);
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  const day = (t: number) => `day ${t}`;
  assert.equal(recordCell(blue, 'tracks', 10, clock, day), '2/10');
  assert.equal(recordCell(blue, 'tracks', null, clock, day), '2', 'the album\'s own count not known');
  assert.equal(recordCell(blue, 'length', null, clock, day), '8:20');
  assert.equal(recordCell(blue, 'year', null, clock, day), '1959');
  assert.equal(recordCell(blue, 'added', null, clock, day), 'day 12');
  assert.equal(recordCell(blue, 'rating', null, clock, day), '', 'a column a record has no value for');
});
