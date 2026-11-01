// Health laid out as Settings (lib/healthThrough; 0.13, mockup C): the page's problems by domain, worst first, and Work
// through: the stops in order (the most missing albums first; damaged files album by album), the next track of a stop
// in track order, skipped or added ones passed, and the count it shows.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { byArea, byDomain, damagedStops, dotOf, handledOne, missingStops, nextStop, nextWanted, skipStop, startThrough, throughKind, throughLine, throughTotal } =
  await import('../src/lib/healthThrough.ts');

test('problems grouped by domain, a domain where its worst problem stands, worst first within', () => {
  const list = [
    { key: 'a', domain: 'Library' },
    { key: 'b', domain: 'Files' },
    { key: 'c', domain: 'Library' },
    { key: 'd', domain: 'Playlists' },
    { key: 'e', domain: 'Files' },
  ];
  assert.deepEqual(
    byDomain(list).map((g) => [g.domain, g.problems.map((p) => p.key)]),
    [
      ['Library', ['a', 'c']],
      ['Files', ['b', 'e']],
      ['Playlists', ['d']],
    ],
  );
});

test("a problem's dot: red where it needs you, amber a warning, grey to know", () => {
  assert.equal(dotOf({ urgent: true, finding: { level: 'critical' } }), 'bad');
  assert.equal(dotOf({ urgent: false, finding: { level: 'warning' } }), 'warn');
  assert.equal(dotOf({ urgent: false, finding: { level: 'info' } }), 'off');
  assert.equal(dotOf({ urgent: true, finding: null }), 'bad');
  assert.equal(dotOf({ urgent: false, finding: null }), 'warn');
});

const album = (id: string, title: string, missing: number) => ({ title, album: ['cratarr', id] as [string, string], missing: Array.from({ length: missing }, (_, i) => `${i + 1} · x`) });

test('Work through: missing tracks, the most missing albums first, then by title', () => {
  const items = [album('1', 'Blue Hours', 1), album('2', 'Late Set', 3), album('3', 'Corner Light', 3), { title: 'Elsewhere', album: ['other', '9'] as [string, string], missing: ['1 · x'] }];
  assert.equal(throughKind(items), 'missing');
  const stops = missingStops(items);
  assert.deepEqual(stops.map((s) => s.title), ['Corner Light', 'Late Set', 'Blue Hours']);
  assert.equal(throughTotal(stops), 7);
});

test('Work through: damaged files album by album, the album with the most first, as listed within', () => {
  const items = [
    { title: 'a1', album: ['cratarr', 'A'] as [string, string], track: 't1' },
    { title: 'b1', album: ['cratarr', 'B'] as [string, string], track: 't2' },
    { title: 'b2', album: ['cratarr', 'B'] as [string, string], track: 't3' },
    { title: 'c1', album: ['cratarr', 'C'] as [string, string], track: 't4' },
  ];
  assert.equal(throughKind(items), 'damaged');
  assert.deepEqual(damagedStops(items).map((s) => s.replace), ['t2', 't3', 't1', 't4']);
  assert.equal(throughKind([{ title: 'x', line: 'nothing to look for' }]), null);
});

const tr = (disc: number, position: number, recording: string, have: string | null = null) => ({ disc, position, recording, have: have ? { source: 'cratarr', id: have } : null });

test("a stop's next track: the first missing in track order, those dealt with passed; done at the end", () => {
  const tracks = [tr(2, 1, 'r4'), tr(1, 3, 'r3'), tr(1, 1, 'r1', 's1'), tr(1, 2, 'r2')];
  const stop = { source: 'cratarr', album: 'A', title: 'A', missing: 3, replace: null };
  assert.equal(nextWanted(tracks, stop, [])?.recording, 'r2');
  assert.equal(nextWanted(tracks, stop, ['r2'])?.recording, 'r3');
  assert.equal(nextWanted(tracks, stop, ['r2', 'r3'])?.recording, 'r4');
  assert.equal(nextWanted(tracks, stop, ['r2', 'r3', 'r4']), null);
});

test("a damaged file's stop wants its own track, once", () => {
  const tracks = [tr(1, 1, 'r1', 's1'), tr(1, 2, 'r2', 's2')];
  const stop = { source: 'cratarr', album: 'A', title: 'A', missing: 1, replace: 's2' };
  assert.equal(nextWanted(tracks, stop, [])?.recording, 'r2');
  assert.equal(nextWanted(tracks, stop, ['r2']), null);
});

test('the walk: each track dealt with counts once, a new stop starts clean, an unreadable stop is passed whole', () => {
  // In the reader's own way of writing a number.
  const all = (1032).toLocaleString();
  let t = startThrough();
  assert.equal(throughLine(t, 1032), `1 of ${all}`);
  t = handledOne(t, 'r2');
  assert.deepEqual(t, { at: 0, handled: ['r2'], count: 1 });
  assert.equal(throughLine(t, 1032), `2 of ${all}`);
  t = nextStop(t);
  assert.deepEqual(t, { at: 1, handled: [], count: 1 });
  t = skipStop(handledOne(t, 'r9'), { source: 'cratarr', album: 'B', title: 'B', missing: 4, replace: null });
  assert.deepEqual(t, { at: 2, handled: [], count: 5 });
  // Never past the last.
  assert.equal(throughLine({ at: 9, handled: [], count: 7 }, 7), '7 of 7');
});

test("Health's three areas, in their order, each its most urgent first; an empty area left out (the owner, 2026-11-01)", () => {
  const p = (key: string, area: string, level: 'critical' | 'warning' | 'info') => ({ key, area, urgent: false, finding: { level } });
  const list = [p('genre', 'Library', 'info'), p('lidarr', 'Connections', 'warning'), p('broken', 'Library', 'critical'), p('cover', 'Library', 'warning')];
  assert.deepEqual(
    byArea(list, ['Library', 'Connections', 'Activity']).map((g) => [g.area, g.problems.map((x) => x.key)]),
    [
      ['Library', ['broken', 'cover', 'genre']],
      ['Connections', ['lidarr']],
    ],
  );
});
