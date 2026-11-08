// The deck machine's places as the page reads them (lib/tape/deck.ts): a track's start, its very end, where the knob
// at L plays it from, and ms in, kept inside the track.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { atMs, FROM_END_MS, signOf } = await import('../src/lib/tape/deck.ts');

test('each place in a track of known length', () => {
  assert.equal(atMs({ at: 'start' }, 120_000), 0);
  assert.equal(atMs({ at: 'end' }, 120_000), 120_000);
  assert.equal(atMs({ at: 'from-end' }, 120_000), 120_000 - FROM_END_MS);
  assert.equal(atMs({ at: 'ms', ms: 60_000 }, 120_000), 60_000);
});

test('a place is kept inside its track', () => {
  assert.equal(atMs({ at: 'ms', ms: -5 }, 120_000), 0);
  assert.equal(atMs({ at: 'ms', ms: 130_000 }, 120_000), 120_000);
  assert.equal(atMs({ at: 'from-end' }, 1_000), 0);
});

test('a track whose length is not known yet: its end is not known either', () => {
  assert.equal(atMs({ at: 'end' }, null), null);
  assert.equal(atMs({ at: 'from-end' }, null), null);
  assert.equal(atMs({ at: 'start' }, null), 0);
  assert.equal(atMs({ at: 'ms', ms: 4_000 }, null), 4_000);
});

test('a way as the head signs it', () => {
  assert.equal(signOf('forwards'), 1);
  assert.equal(signOf('backwards'), -1);
});

const { eventCode, doOf } = await import('../src/lib/tape/deck.ts');

test('events in the engine\'s numbers (the rules crate\'s wasm.rs)', () => {
  assert.deepEqual(eventCode({ kind: 'play' }), [0, 0, 0]);
  assert.deepEqual(eventCode({ kind: 'hold-start', way: 'backwards' }), [4, 1, 0]);
  assert.deepEqual(eventCode({ kind: 'turn', knob: 'L' }), [6, 1, 0]);
  assert.deepEqual(eventCode({ kind: 'seek', to: 61_234.6, wind: true }), [7, 61_235, 1]);
  assert.deepEqual(eventCode({ kind: 'refused', why: 'unreachable' }), [10, 1, 0]);
  assert.deepEqual(eventCode({ kind: 'new-queue', play: false }), [13, 0, 0]);
});

/** A thing to do as the engine gives it, field by field (-1 for a field it has not). */
const fields = (f: number[]) => (i: number) => f[i] ?? -1;

test('things to do from the engine\'s fields', () => {
  assert.deepEqual(doOf(fields([0, 1])), { do: 'play', way: 'backwards' });
  assert.deepEqual(doOf(fields([2, 0, 3, 90_000, -1, 0, 1])), { do: 'tap', to: { at: 'ms', ms: 90_000 }, way: 'forwards', fromStill: false, landsStill: true });
  assert.deepEqual(doOf(fields([3, 1, -1, -1, -1, 1])), { do: 'hold', way: 'backwards', fromStill: true });
  assert.deepEqual(doOf(fields([4, -1, -1, -1, -1, 1])), { do: 'let-go', toOne: true });
  assert.deepEqual(doOf(fields([5, -1, 2, 0, 3])), { do: 'go-to', track: 3, at: { at: 'from-end' } });
  assert.deepEqual(doOf(fields([6, -1, 1, 0, 0])), { do: 'cross', track: 0, at: { at: 'end' } });
  assert.deepEqual(doOf(fields([7])), { do: 'stand' });
  assert.deepEqual(doOf(fields([8, 1])), { do: 'say', why: 'unreachable' });
  assert.equal(doOf(fields([9])), null);
});
