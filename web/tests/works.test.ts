import { test } from 'node:test';
import assert from 'node:assert/strict';
import { firstRecording, groupByWork, inPartOrder, movementLabel, workLine, worksInOrder } from '../src/lib/works.ts';

const fifth = { mbid: 'w-5', title: 'Symphony No. 5 in C minor, Op. 67' };
const row = (name: string, part?: string, parent: { mbid: string; title: string } | null = fifth, number?: number) => ({
  name,
  work: part ? { mbid: `p-${part}`, title: `${parent?.title ?? 'x'}: ${part}`, parent, part_number: number ?? null } : null,
});

test('a composer\'s works: the most recorded first, then by title', () => {
  const l = worksInOrder([
    { work: 'a', title: 'Cello Suite No. 2', parts: 6, recordings: 1 },
    { work: 'b', title: 'Goldberg Variations', parts: 32, recordings: 3 },
    { work: 'c', title: 'Cello Suite No. 10', parts: 6, recordings: 1 },
    { work: 'd', title: 'Cello Suite No. 1', parts: 6, recordings: 1 },
  ]);
  assert.deepEqual(l.map((w) => w.work), ['b', 'd', 'a', 'c']);
  assert.equal(workLine({ recordings: 3, catalogue: 'BWV 988', key: 'G major' }), '3 recordings · BWV 988 · G major');
  assert.equal(workLine({ recordings: 1 }), '1 recording');
});

test('a movement is named by its movement only, its numbering as the tags have it', () => {
  assert.equal(movementLabel('Symphony No. 5 in C minor, Op. 67: I. Allegro con brio', fifth.title), 'I. Allegro con brio');
  assert.equal(movementLabel('Symphony no. 5 - II. Andante con moto', fifth.title), 'II. Andante con moto');
  assert.equal(movementLabel('Symphonie Nr. 5: 3. Allegro', 'Symphonie Nr. 5 c-Moll op. 67'), '3. Allegro');
  // The track's title says nothing of the work: its part's title does.
  assert.equal(movementLabel('Allegro', fifth.title, 'Symphony No. 5 in C minor, Op. 67: IV. Allegro'), 'IV. Allegro');
  // Another symphony's title is not this one's short form: kept whole.
  assert.equal(movementLabel('Symphony No. 9: IV. Presto', fifth.title), 'Symphony No. 9: IV. Presto');
  // Neither: the track's title as it is.
  assert.equal(movementLabel('Prelude in C', fifth.title, null), 'Prelude in C');
});

test('an album: movements of one work under its heading; a lone movement, no work or no parent yet, as they are', () => {
  const rows = [
    row('Coriolan Overture, Op. 62'),
    row('Symphony No. 5 in C minor, Op. 67: I. Allegro con brio', 'I. Allegro con brio'),
    row('Symphony No. 5 in C minor, Op. 67: II. Andante con moto', 'II. Andante con moto'),
    row('Egmont: Overture', 'Overture', { mbid: 'w-egmont', title: 'Egmont, Op. 84' }),
    { name: 'Encore', work: { mbid: 'p-x', title: 'Encore', parent: null } },
  ];
  const lines = groupByWork(rows);
  assert.deepEqual(
    lines.map((l) => (l.kind === 'work' ? `[${l.work.title}: ${l.at.join(',')}]` : `${l.at}${l.movement ? ` ${l.movement}` : ''}`)),
    ['0', `[${fifth.title}: 1,2]`, '1 I. Allegro con brio', '2 II. Andante con moto', '3', '4'],
  );
});

test('a work\'s movements split by another track: two headings, never one over a stranger', () => {
  const rows = [row('a: I', 'I'), row('a: II', 'II'), row('Interlude', undefined, null), row('a: III', 'III'), row('a: IV', 'IV')];
  const heads = groupByWork(rows).filter((l) => l.kind === 'work');
  assert.equal(heads.length, 2);
  assert.deepEqual(heads.map((h) => (h.kind === 'work' ? h.at : [])), [[0, 1], [3, 4]]);
});

test('a recording plays in its work\'s order, unknown parts keeping their place', () => {
  const parts = [
    { mbid: 'p3', number: 3 },
    { mbid: 'p1', number: 1 },
    { mbid: 'p2', number: 2 },
  ];
  const tracks = [{ id: 'c', part: 'p3' }, { id: 'x', part: null }, { id: 'a', part: 'p1' }, { id: 'b', part: 'p2' }];
  assert.deepEqual(inPartOrder(tracks, parts).map((t) => t.id), ['a', 'b', 'c', 'x']);
  assert.deepEqual(inPartOrder([{ id: 'y' }, { id: 'z' }], parts).map((t) => t.id), ['y', 'z']);
});

test("a work found plays its first recording, in the work's order; none without one", () => {
  const parts = [
    { mbid: 'p2', number: 2 },
    { mbid: 'p1', number: 1 },
  ];
  const w = { parts, recordings: [{ tracks: [{ id: 'b', part: 'p2' }, { id: 'a', part: 'p1' }] }, { tracks: [{ id: 'z', part: 'p1' }] }] };
  assert.deepEqual(firstRecording(w), ['a', 'b']);
  assert.deepEqual(firstRecording({ parts, recordings: [] }), []);
});
