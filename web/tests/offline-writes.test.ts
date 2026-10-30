// Offline writes, the page's rules (0.12, decisions 0085 step 3 and 0087): the outbox (lib/outbox: what an answer
// means, the headers a change goes with, two copies merged), the change log read (lib/changeLog), and a playlist's
// edits as operations (lib/playlistOps: the fewest that make the edit, and their merge as Cratarr makes it).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { outcomeOf, headersOf, mergeOps, opsFrom, stale, STALE_MS } = await import('../src/lib/outbox.ts');
const { readChanges, favouriteOf } = await import('../src/lib/changeLog.ts');
const { opsFor, applyOps } = await import('../src/lib/playlistOps.ts');

const notReached = (s: number) => s === 502 || s === 503 || s === 504;
const op = (key: string, at: number) => ({ key, method: 'POST', path: `/api/x/${key}`, body: null, type: null, at });

test('what an answer means for the change at the head', () => {
  assert.equal(outcomeOf(null, false, notReached), 'wait');
  assert.equal(outcomeOf(200, false, notReached), 'sent');
  assert.equal(outcomeOf(204, false, notReached), 'sent');
  assert.equal(outcomeOf(409, true, notReached), 'again');
  assert.equal(outcomeOf(409, false, notReached), 'refused');
  assert.equal(outcomeOf(404, false, notReached), 'refused');
  assert.equal(outcomeOf(401, false, notReached), 'wait');
  assert.equal(outcomeOf(503, false, notReached), 'wait');
  assert.equal(outcomeOf(500, false, notReached), 'wait');
});

test('a change goes with its op id and its time, first try and resend alike', () => {
  const o = { ...op('k1', 1_700_000_000_123), type: 'application/json' };
  assert.deepEqual(headersOf(o), { 'Idempotency-Key': 'k1', 'X-Cratarr-At': '1700000000123', 'Content-Type': 'application/json' });
  assert.deepEqual(headersOf(op('k2', 5)), { 'Idempotency-Key': 'k2', 'X-Cratarr-At': '5' });
});

test('two copies of the outbox as one: each change once, in the order made', () => {
  const a = [op('a', 10), op('c', 30)];
  const b = [op('b', 20), op('a', 10), op('d', 30)];
  assert.deepEqual(
    mergeOps(a, b).map((x) => x.key),
    ['a', 'b', 'c', 'd'],
  );
  assert.deepEqual(mergeOps([], []), []);
});

test('a list read back: only well-formed changes', () => {
  assert.deepEqual(opsFrom(null), []);
  assert.deepEqual(opsFrom({ list: [] }), []);
  assert.deepEqual(opsFrom([op('a', 1), { key: 2 }, null, 'x']).map((x) => x.key), ['a']);
});

test('a week waiting is let go', () => {
  assert.equal(stale(op('a', 0), STALE_MS), false);
  assert.equal(stale(op('a', 0), STALE_MS + 1), true);
});

test('the change log: ids per kind, kinds not asked passed over, pages added up', () => {
  const kinds = ['playlist', 'favourite', 'rating'];
  const first = readChanges(kinds, {
    rev: 10,
    more: true,
    changes: [
      { rev: 8, kind: 'playlist', id: 'Jazz', op: 'changed' },
      { rev: 9, kind: 'excluded', id: 'x', op: 'changed' },
      { rev: 10, kind: 'favourite', id: 'track:cratarr/tr-1', op: 'changed' },
    ],
  });
  assert.equal(first.rev, 10);
  assert.equal(first.more, true);
  assert.deepEqual([...first.refresh.keys()].sort(), ['favourite', 'playlist']);
  const second = readChanges(kinds, { rev: 12, changes: [{ rev: 12, kind: 'playlist', id: 'Rock', op: 'deleted' }] }, first.refresh);
  assert.equal(second.rev, 12);
  assert.equal(second.more, false);
  assert.deepEqual([...(second.refresh.get('playlist') as Set<string>)].sort(), ['Jazz', 'Rock']);
});

test('the change log: too old a rev reads every kind again', () => {
  const r = readChanges(['playlist', 'rating'], { rev: 99, reset: true });
  assert.equal(r.rev, 99);
  assert.equal(r.refresh.get('playlist'), 'all');
  assert.equal(r.refresh.get('rating'), 'all');
  // A reset after ids seen: all wins.
  const m = new Map([['playlist', new Set(['Jazz'])]]);
  readChanges(['playlist'], { rev: 3, reset: true }, m);
  assert.equal(m.get('playlist'), 'all');
});

test("a favourite's id in the log", () => {
  assert.deepEqual(favouriteOf('track:cratarr/tr-1'), { what: 'track', source: 'cratarr', id: 'tr-1' });
  assert.deepEqual(favouriteOf('album:jf/al/2'), { what: 'album', source: 'jf', id: 'al/2' });
  assert.deepEqual(favouriteOf('artist:cratarr/Ann Quartet'), { what: 'artist', source: 'cratarr', id: 'Ann Quartet' });
  assert.deepEqual(favouriteOf('playlist:Road trip'), { what: 'playlist', id: 'Road trip' });
  assert.equal(favouriteOf('crate:x'), null);
  assert.equal(favouriteOf('track:cratarr'), null);
  assert.equal(favouriteOf('nothing'), null);
});

let n = 0;
const id = () => `op${++n}`;
const ids = (xs: { entry: string }[]) => xs.map((x) => x.entry);
const rows = (...es: string[]) => es.map((entry) => ({ entry }));
const play = (before: string[], after: string[]) => {
  const ops = opsFor(before, after, 1, id);
  const made = applyOps(rows(...before), ops, (r) => r.entry, (o) => ({ entry: o.entry }));
  return { ops, made: ids(made) };
};

test('a playlist edit as operations: removals', () => {
  const { ops, made } = play(['a', 'b', 'c', 'd'], ['a', 'c']);
  assert.deepEqual(made, ['a', 'c']);
  assert.deepEqual(
    ops.map((o) => [o.op, o.entry]),
    [
      ['remove', 'b'],
      ['remove', 'd'],
    ],
  );
});

test('a playlist edit as operations: the fewest moves', () => {
  // One line to the top: one move.
  let r = play(['a', 'b', 'c', 'd', 'e'], ['d', 'a', 'b', 'c', 'e']);
  assert.deepEqual(r.made, ['d', 'a', 'b', 'c', 'e']);
  assert.equal(r.ops.length, 1);
  assert.deepEqual(r.ops[0], { op_id: r.ops[0].op_id, at: 1, op: 'move', entry: 'd', after: null });
  // Reversed: all but one moved.
  r = play(['a', 'b', 'c', 'd'], ['d', 'c', 'b', 'a']);
  assert.deepEqual(r.made, ['d', 'c', 'b', 'a']);
  assert.equal(r.ops.length, 3);
  // Unchanged: nothing.
  assert.deepEqual(play(['a', 'b'], ['a', 'b']).ops, []);
  // Moved and removed at once.
  r = play(['a', 'b', 'c', 'd', 'e'], ['e', 'b', 'd']);
  assert.deepEqual(r.made, ['e', 'b', 'd']);
  assert.equal(r.ops.filter((o) => o.op === 'move').length, 1);
});

test("operations merged as Cratarr merges them: set membership, an edit's ops replayed twice change nothing more", () => {
  const ops = opsFor(['a', 'b', 'c'], ['c', 'a'], 1, id);
  const once = applyOps(rows('a', 'b', 'c'), ops, (r) => r.entry, (o) => ({ entry: o.entry }));
  const twice = applyOps(once, ops, (r) => r.entry, (o) => ({ entry: o.entry }));
  assert.deepEqual(ids(twice), ['c', 'a']);
  // Removing a line already gone elsewhere, moving after one gone: the rest still lands.
  const other = applyOps(rows('a', 'c', 'x'), ops, (r) => r.entry, (o) => ({ entry: o.entry }));
  assert.deepEqual(ids(other), ['c', 'a', 'x']);
  // An add: once, after its line; again, nothing.
  const add = { op_id: 'n', at: 2, op: 'add' as const, entry: 'n', track: { source: 'cratarr', id: 'tr-9' }, after: 'a' };
  const added = applyOps(rows('a', 'b'), [add, add], (r) => r.entry, (o) => ({ entry: o.entry }));
  assert.deepEqual(ids(added), ['a', 'n', 'b']);
});
