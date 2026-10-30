// Queue requests in the queue (lib/queueRequests; the owner, 2026-10-30): where each shows, whether they show, and how
// long one accepted stands in for the player's report.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { ACCEPTED_MS, requestsShown, stillComing, withRequests } = await import('../src/lib/queueRequests.ts');

const t = (id: string, lane?: string) => ({ id, lane });
const r = (id: string, how: 'next' | 'last', at: number) => ({ id, from: { user: 'ann' }, track: { source: 'cratarr', id, name: id, artist: null, album: null, album_id: null }, how, at, player: 'Tab' });
const shape = (lines: ReturnType<typeof withRequests>) => lines.map((l) => (l.kind === 'track' ? (l.t as { id: string }).id : `${l.kind}:${l.r.id}`));

test('one for next after the next lane, one for the end before Autoplay, oldest first', () => {
  const up = [t('n1', 'next'), t('n2', 'next'), t('a'), t('rd', 'radio'), t('b'), t('ap', 'autoplay')];
  assert.deepEqual(shape(withRequests(up, [r('x', 'last', 2), r('y', 'next', 3), r('z', 'next', 1)])), ['n1', 'n2', 'request:z', 'request:y', 'a', 'rd', 'b', 'request:x', 'ap']);
});

test('an empty queue, and one of only the next lane or only Autoplay', () => {
  assert.deepEqual(shape(withRequests([], [r('x', 'last', 1), r('y', 'next', 2)])), ['request:y', 'request:x']);
  assert.deepEqual(shape(withRequests([t('n', 'next')], [r('x', 'last', 1)])), ['n', 'request:x']);
  assert.deepEqual(shape(withRequests([t('ap', 'autoplay')], [r('x', 'last', 1), r('y', 'next', 2)])), ['request:y', 'request:x', 'ap']);
});

test('the queue keeps its own numbering under the rows put between', () => {
  const lines = withRequests([t('a'), t('b')], [r('x', 'next', 1)]);
  assert.deepEqual(lines.flatMap((l) => (l.kind === 'track' ? [l.i] : [])), [0, 1]);
});

test('one accepted shows in its place, before those still asked for at the same time', () => {
  assert.deepEqual(shape(withRequests([t('a')], [r('y', 'last', 5)], [r('x', 'last', 5)])), ['a', 'accepted:x', 'request:y']);
});

test('on while they let others follow them, unless this device chose', () => {
  assert.equal(requestsShown(null, true), true);
  assert.equal(requestsShown(null, false), false);
  assert.equal(requestsShown(false, true), false);
  assert.equal(requestsShown(true, false), true);
});

test('one accepted stands in until the queue has its track once more, and not for long', () => {
  const a = { r: r('x', 'last', 0), before: 1, at: 1000 };
  assert.equal(stillComing(a, [{ id: 'x' }], 2000), true, 'the queue held it once already');
  assert.equal(stillComing(a, [{ id: 'x' }, { id: 'x' }], 2000), false, 'in the report now');
  assert.equal(stillComing(a, [], 1000 + ACCEPTED_MS), false, 'gone after a while');
});
