// The tape's ramp as one value (lib/player/rampSlot), over the sequences the player runs it through (lib/localPlayer):
// a wind-up started early and ended, a pause pressed during it, a seek during a ramp, a skip's wind-down.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { rampSlot } = await import('../src/lib/player/rampSlot.ts');

test('a wind-up: committed to playing, the counter held where it was pressed, both gone at its end', () => {
  const t = rampSlot();
  t.committed = false;
  t.heldAt = 12.5;
  assert.deepEqual(t.ramp, { committed: false, heldAt: 12.5 });
  // Its end: rampEnds, the counter let go, then no ramp.
  t.heldAt = null;
  t.committed = null;
  assert.equal(t.ramp, null);
  assert.equal(t.heldAt, null);
});

test('a pause pressed during the wind-up turns it round, keeping the counter where the wind-up was going', () => {
  const t = rampSlot();
  t.committed = false;
  t.heldAt = 30;
  t.committed = true;
  assert.equal(t.heldAt, 30, 'the turn keeps the counter');
  if (t.heldAt === null) t.heldAt = 31;
  assert.deepEqual(t.ramp, { committed: true, heldAt: 30 });
});

test('a seek during a ramp, a skip, a stop at once: the counter goes with the ramp', () => {
  const t = rampSlot();
  t.committed = false;
  t.heldAt = 5;
  t.committed = null;
  assert.equal(t.heldAt, null, 'no ramp, no counter held');
});

test('a counter cannot be held with no ramp', () => {
  const t = rampSlot();
  t.heldAt = 7;
  assert.equal(t.heldAt, null);
  assert.equal(t.ramp, null);
});
