// The tape's state (lib/player/tape): its phase as read from what the player holds, over a hold's press, wind, let-go
// and brake, a ramp, a plain hold and a glide.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { phaseOf } = await import('../src/lib/player/tape.ts');
type V = Parameters<typeof phaseOf>[0];
const still: V = { committed: null, rewinding: null, holding: null, plainHold: null, glide: null, shownRamp: null, knob: 'forward' };

test('a hold: pressed (arming), winding, let go (braking), landed (idle)', () => {
  const press: V = { ...still, holding: { dir: 1, released: false } };
  assert.equal(phaseOf(press), 'arming');
  const wind: V = { ...press, committed: true, rewinding: { brake: null, reverse: false } };
  assert.equal(phaseOf(wind), 'winding');
  const brake: V = { ...wind, holding: { dir: 1, released: true }, rewinding: { brake: 1234, reverse: false } };
  assert.equal(phaseOf(brake), 'braking');
  assert.equal(phaseOf(still), 'idle');
});

test('a ramp, a plain hold, a glide', () => {
  assert.equal(phaseOf({ ...still, committed: false, shownRamp: 'out' }), 'ramp');
  assert.equal(phaseOf({ ...still, plainHold: { dir: 1, speeding: true } }), 'plainHold');
  assert.equal(phaseOf({ ...still, glide: { from: 0, to: 1, at: 0, span: 1 } }), 'gliding');
});
