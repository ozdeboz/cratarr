// The rest of the tape's state (lib/player/tapeParts): the tape's own ramp, a Next waiting for its moment, a hold on the
// player driven, each through its own transitions, over the sequences the player runs them through.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { ownRamp, NO_OWN_RAMP, pendingNext, NO_PENDING_NEXT, drivenHold, NO_DRIVEN_HOLD } = await import('../src/lib/player/tapeParts.ts');

test("the tape's own ramp: a turn (both), a skip (quiet only), a handover's turn without quiet", () => {
  assert.deepEqual(ownRamp(true, { ms: 400 }), { quiet: true, turn: { ms: 400 } });
  assert.deepEqual(ownRamp(true, null), { quiet: true, turn: null });
  assert.deepEqual(ownRamp(false, { ms: 400 }), { quiet: false, turn: { ms: 400 } });
  assert.deepEqual(NO_OWN_RAMP, { quiet: false, turn: null });
});

test('a Next for the standstill and one for Autoplay may both wait, each ended by its own moment', () => {
  let p = pendingNext.whenFilled(NO_PENDING_NEXT);
  p = pendingNext.afterPause(p);
  assert.deepEqual(p, { afterPause: true, whenFilled: true });
  p = pendingNext.pauseDone(p);
  assert.deepEqual(p, { afterPause: false, whenFilled: true }, "the brake stood: Autoplay's still waits");
  assert.deepEqual(pendingNext.filled(p), NO_PENDING_NEXT);
});

test('a hold on the player driven: pressed, let go, landed', () => {
  let h = drivenHold.press();
  assert.equal(drivenHold.landing(h), false, 'still pressed: the player saying it stopped does not end it');
  h = drivenHold.letGo(h);
  assert.deepEqual(h, { held: true, letGo: true });
  assert.equal(drivenHold.landing(h), true);
  h = drivenHold.over(h);
  assert.equal(h.held, false);
  assert.equal(drivenHold.landing(h), false);
  assert.deepEqual(drivenHold.press(), { held: true, letGo: false }, 'pressed again: anew');
  assert.deepEqual(NO_DRIVEN_HOLD, { held: false, letGo: false });
});
