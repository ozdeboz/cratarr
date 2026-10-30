// The Downloads shelf (lib/shelf, decision 0085 step 2): records follow what a place holds, details are filled in by
// album once, search reads what was filled, and albums group and order as Downloads shows them, all with no network.
import { test } from 'node:test';
import assert from 'node:assert/strict';
await import('./with-matcher.ts');

const { recordOf, plan, toFill, filled, shelfMatches, albumsOf, albumMatches, orderAlbums, coverSize, keyOf, REFILL_MS, COVER_PX } = await import('../src/lib/shelf.ts');

const held = (id: string, album_id: string | null = 'al-1', more: Record<string, unknown> = {}) => ({
  source: 'cratarr',
  id,
  name: `Track ${id}`,
  artist: 'Ann',
  album: album_id ? 'Blue Hours' : null,
  album_id,
  ...more,
});

const view = {
  title: 'Blue Hours',
  artist: 'Ann Quartet',
  year: 1975,
  genres: ['Jazz', 'Soul Jazz'],
  rows: [
    { id: 'tr-2', name: 'Two', artist: 'Ann', duration_secs: 200, disc: 1, track: 2 },
    { id: 'tr-1', name: 'One', artist: 'Ann', duration_secs: 180, disc: 1, track: 1 },
  ],
};

test('a place followed: new tracks in, gone ones out, another place left alone, nothing written twice', () => {
  const first = plan([], 'app', [held('tr-1'), held('tr-2'), held('tr-1')], 1000);
  assert.deepEqual(first.put.map((r) => r.key), ['app:cratarr/tr-1', 'app:cratarr/tr-2']);
  assert.deepEqual(first.remove, []);
  const browser = recordOf('browser', held('tr-9'), undefined, 500);
  const kept = [...first.put, browser];
  // Unchanged: nothing to write.
  assert.deepEqual(plan(kept, 'app', [held('tr-1'), held('tr-2')], 2000), { put: [], remove: [] });
  // tr-2 removed on the device: it goes; the browser's record stays.
  const next = plan(kept, 'app', [held('tr-1')], 3000);
  assert.deepEqual(next.remove, ['app:cratarr/tr-2']);
  assert.deepEqual(next.put, []);
});

test('a record keeps what was filled in and when it came; a new album id is read again', () => {
  const r = recordOf('app', held('tr-1'), undefined, 1000);
  const [f] = filled([r], 'cratarr', 'al-1', view, 5000);
  const again = recordOf('app', held('tr-1'), f, 9000);
  assert.equal(again.added, 1000);
  assert.equal(again.year, 1975);
  assert.equal(again.filled, 5000);
  assert.deepEqual(again.genres, ['Jazz', 'Soul Jazz']);
  const moved = recordOf('app', held('tr-1', 'al-2'), f, 9000);
  assert.equal(moved.filled, 0);
});

test('albums to fill: one each, the oldest first, none without an album, again after REFILL_MS', () => {
  const a = recordOf('app', held('tr-1', 'al-1'), undefined, 300);
  const b = recordOf('app', held('tr-2', 'al-1'), undefined, 100);
  const c = recordOf('app', held('tr-3', 'al-2'), undefined, 200);
  const loose = recordOf('app', held('tr-4', null), undefined, 50);
  assert.deepEqual(toFill([a, b, c, loose], 1000), [
    { source: 'cratarr', album_id: 'al-1' },
    { source: 'cratarr', album_id: 'al-2' },
  ]);
  const done = filled([a, b], 'cratarr', 'al-1', view, 1000);
  assert.deepEqual(toFill([...done, c], 2000), [{ source: 'cratarr', album_id: 'al-2' }]);
  assert.deepEqual(toFill(done, 1000 + REFILL_MS + 1), [{ source: 'cratarr', album_id: 'al-1' }]);
});

test("filled from the album's view: each track by id, the album's details for one not found", () => {
  const recs = [recordOf('app', held('tr-1'), undefined, 1), recordOf('app', held('tr-3'), undefined, 1), recordOf('app', held('tr-x', 'al-9'), undefined, 1)];
  const out = filled(recs, 'cratarr', 'al-1', view, 42);
  assert.equal(out.length, 2, 'only that album');
  const one = out.find((r) => r.id === 'tr-1')!;
  assert.equal(one.track, 1);
  assert.equal(one.duration_secs, 180);
  assert.equal(one.album_artist, 'Ann Quartet');
  const missing = out.find((r) => r.id === 'tr-3')!;
  assert.equal(missing.track, null);
  assert.equal(missing.year, 1975);
  assert.equal(missing.filled, 42);
});

test('search reads name, artist, album, genres and year', () => {
  const [r] = filled([recordOf('app', held('tr-1'), undefined, 1)], 'cratarr', 'al-1', view, 2);
  assert.ok(shelfMatches(r, 'soul jazz'));
  assert.ok(shelfMatches(r, '1975'));
  assert.ok(shelfMatches(r, 'blue hours'));
  assert.ok(shelfMatches(r, 'ann quartet'));
  assert.ok(!shelfMatches(r, 'techno'));
});

test('albums group by album, tracks in album order; a loose track is its own; search and order', () => {
  const recs = filled([recordOf('app', held('tr-2'), undefined, 10), recordOf('app', held('tr-1'), undefined, 20)], 'cratarr', 'al-1', view, 30);
  const other = { ...recordOf('app', held('tr-5', 'al-3', { album: 'Alpha', artist: 'Robin' }), undefined, 40), year: 1990, album_artist: 'Robin' };
  const loose = recordOf('app', held('tr-6', null), undefined, 5);
  const albums = albumsOf([...recs, other, loose]);
  assert.equal(albums.length, 3);
  const blue = albums.find((a) => a.album_id === 'al-1')!;
  assert.deepEqual(blue.tracks.map((t) => t.id), ['tr-1', 'tr-2']);
  assert.equal(blue.duration_secs, 380);
  assert.equal(blue.added, 20);
  assert.ok(albumMatches(blue, 'track tr-2'), 'a track name finds its album');
  assert.ok(!albumMatches(blue, 'robin'));
  assert.ok(albumMatches(blue, ''), 'no words: every album');
  assert.deepEqual(orderAlbums(albums, 'added').map((a) => a.album_id ?? a.tracks[0].id), ['al-3', 'al-1', 'tr-6']);
  assert.deepEqual(orderAlbums(albums, 'title').map((a) => a.title), ['Alpha', 'Blue Hours', 'Track tr-6']);
  assert.deepEqual(orderAlbums(albums, 'year').map((a) => a.year), [1990, 1975, null]);
});

test('a cover made small keeps its shape, never grows', () => {
  assert.deepEqual(coverSize(1200, 1200), { w: COVER_PX, h: COVER_PX });
  assert.deepEqual(coverSize(1000, 500), { w: COVER_PX, h: 80 });
  assert.deepEqual(coverSize(100, 80), { w: 100, h: 80 });
  assert.equal(keyOf('browser', { source: 's', id: 'i' }), 'browser:s/i');
});
