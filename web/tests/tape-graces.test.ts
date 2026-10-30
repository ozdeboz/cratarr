// The tape's passing states (lib/player/graces): a deadline set as each is entered, kept while it stands, dropped
// as it is left, and due once past.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { entered, due, soonest, GRACE_MS, NO_DEADLINES } = await import('../src/lib/player/graces.ts');
const none = { shownRamp: false, knobTurn: false, idleHold: false, quietRest: null };

test("a hold whose wind has not begun: its deadline from the moment it stood, due past it, gone once it winds", () => {
  let d = entered(NO_DEADLINES, { ...none, idleHold: true }, 1000);
  assert.equal(d.idleHold, 1000 + GRACE_MS.idleHold);
  d = entered(d, { ...none, idleHold: true }, 3000);
  assert.equal(d.idleHold, 1000 + GRACE_MS.idleHold, 'still standing: the same deadline');
  assert.deepEqual(due(d, 1000 + GRACE_MS.idleHold), [], 'not past it yet');
  assert.deepEqual(due(d, 1001 + GRACE_MS.idleHold), ['idleHold']);
  assert.equal(entered(d, none, 3500).idleHold, null, 'its wind began: no deadline');
});

test('a ramp shown while the app says nothing yet, and a turn of the knob', () => {
  const d = entered(NO_DEADLINES, { ...none, shownRamp: true, knobTurn: true }, 0);
  assert.equal(soonest(d), GRACE_MS.shownRamp);
  assert.deepEqual(due(d, GRACE_MS.shownRamp + 1), ['shownRamp']);
  assert.deepEqual(due(d, GRACE_MS.knobTurn + 1), ['shownRamp', 'knobTurn']);
});

test("the tape's quiet rest ends at its own deadline", () => {
  const d = entered(NO_DEADLINES, { ...none, quietRest: 5000 }, 4000);
  assert.equal(soonest(d), 5000);
  assert.deepEqual(due(d, 5001), ['quietRest']);
  assert.equal(soonest(entered(d, none, 4500)), null, 'a ramp or a wind took over: no rest to end');
});
