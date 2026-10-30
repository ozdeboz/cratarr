// The tape's state (lib/player/tape): its phase as read from what the player holds, and its rules over sequences of
// moments (a hold, its let-go and brake, a ramp, the knob turned), each broken one with its mend.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { phaseOf, tapeRules, GRACE } = await import('../src/lib/player/tape.ts');
type V = Parameters<typeof phaseOf>[0];
const still: V = { committed: null, heldAt: null, rewinding: null, holding: null, plainHold: null, glide: null, shownRamp: null, knob: 'forward' };
const none = { shownRamp: 0, knobTurn: 0, idleHold: 0 };

test('a hold: pressed (arming), winding, let go (braking), landed (idle)', () => {
  const press: V = { ...still, holding: { dir: 1, released: false } };
  assert.equal(phaseOf(press), 'arming');
  const wind: V = { ...press, committed: true, rewinding: { brake: null, reverse: false } };
  assert.equal(phaseOf(wind), 'winding');
  const brake: V = { ...wind, holding: { dir: 1, released: true }, rewinding: { brake: 1234, reverse: false } };
  assert.equal(phaseOf(brake), 'braking');
  assert.equal(phaseOf(still), 'idle');
  for (const v of [press, wind, brake, still]) assert.deepEqual(tapeRules(v, none, 0).broken, [], 'no rule broken on the way');
});

test('backwards: the knob turned, the tape playing backwards, back to forward', () => {
  const back: V = { ...still, committed: true, knob: 'backward', rewinding: { brake: null, reverse: true } };
  assert.equal(phaseOf(back), 'winding');
  assert.deepEqual(tapeRules(back, none, 0).broken, []);
  assert.deepEqual(tapeRules({ ...still, knob: 'backward' }, none, 0).broken.map((b) => b.fix), ['knobForward'], 'backwards with nothing playing so');
});

test('a ramp, a plain hold, a glide', () => {
  assert.equal(phaseOf({ ...still, committed: false, shownRamp: 'out' }), 'ramp');
  assert.equal(phaseOf({ ...still, plainHold: { dir: 1, speeding: true } }), 'plainHold');
  assert.equal(phaseOf({ ...still, glide: { from: 0, to: 1, at: 0, span: 1 } }), 'gliding');
});

test('what may not hold together is broken at once, with its mend', () => {
  const r = tapeRules({ ...still, heldAt: 12, rewinding: { brake: null, reverse: false }, knob: 'backward' }, none, 0).broken;
  assert.deepEqual(
    r.map((b) => b.fix),
    ['none', 'unholdCounter', 'knobForward'],
  );
});

test('a passing state is broken only once it has stood past its grace, then counted afresh', () => {
  const shown: V = { ...still, shownRamp: 'in' };
  let s = tapeRules(shown, none, 1000);
  assert.deepEqual(s.broken, []);
  assert.equal(s.since.shownRamp, 1000);
  s = tapeRules(shown, s.since, 1000 + GRACE.shownRamp);
  assert.deepEqual(s.broken, [], 'not past it yet');
  s = tapeRules(shown, s.since, 1001 + GRACE.shownRamp);
  assert.deepEqual(
    s.broken.map((b) => b.fix),
    ['endShownRamp'],
  );
  assert.equal(s.since.shownRamp, 0);
  assert.equal(tapeRules(still, { ...none, shownRamp: 500 }, 9000).since.shownRamp, 0, 'gone: no longer counted');
  const hold: V = { ...still, holding: { dir: -1, released: false } };
  const h = tapeRules(hold, tapeRules(hold, none, 1).since, GRACE.idleHold + 2);
  assert.deepEqual(
    h.broken.map((b) => b.fix),
    ['dropHold'],
  );
  const turn: V = { ...still, knob: 'stopping' };
  assert.deepEqual(
    tapeRules(turn, tapeRules(turn, none, 1).since, GRACE.knobTurn + 2).broken.map((b) => b.fix),
    ['knobForward'],
  );
});
