// The tape's motion as one value (lib/player/motionSlot), over the sequences the player runs: a hold's wind and its
// end, a plain hold on a track that cannot be wound, the app's wind gliding between reports, and one motion replacing
// another.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { motionSlot } = await import('../src/lib/player/motionSlot.ts');
type W = { id: string };
type P = { dir: 1 | -1; speeding: boolean };
type G = { from: number; to: number };

test('a wind sounds, then ends: no motion', () => {
  const t = motionSlot<W, P, G>();
  t.rewinding = { id: 'w1' };
  assert.deepEqual(t.motion, { kind: 'wind', wind: { id: 'w1' } });
  assert.equal(t.plainHold, null);
  t.rewinding = null;
  assert.equal(t.motion, null);
});

test('a plain hold, let go', () => {
  const t = motionSlot<W, P, G>();
  t.plainHold = { dir: 1, speeding: true };
  assert.equal(t.rewinding, null);
  t.plainHold = null;
  assert.equal(t.motion, null);
});

test("the app's wind: each report glides on from the last, a report off the wind ends it", () => {
  const t = motionSlot<W, P, G>();
  t.glide = { from: 10, to: 8 };
  t.glide = { from: 8, to: 6 };
  assert.deepEqual(t.glide, { from: 8, to: 6 });
  t.glide = null;
  assert.equal(t.motion, null);
});

test('a motion begun replaces any other; one ended ends only itself', () => {
  const t = motionSlot<W, P, G>();
  t.glide = { from: 1, to: 2 };
  t.rewinding = { id: 'w' };
  assert.equal(t.glide, null);
  t.plainHold = null;
  assert.deepEqual(t.rewinding, { id: 'w' }, 'ending a plain hold that is not there leaves the wind');
});
