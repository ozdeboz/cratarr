// The tape over recorded sequences (lib/player rampSlot, motionSlot, graces, tape's phase), each as lib/localPlayer
// runs it: a wind to a point, a hold and its let-go and brake to a stand, backwards, a hold through a track's edge, a
// seek and a pause in a wind, a track ending in a hold, the page's own player against the app's.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { rampSlot } = await import('../src/lib/player/rampSlot.ts');
const { motionSlot } = await import('../src/lib/player/motionSlot.ts');
const { entered, due, GRACE_MS, NO_DEADLINES } = await import('../src/lib/player/graces.ts');
const { phaseOf } = await import('../src/lib/player/tape.ts');

type W = { brake: number | null; reverse: boolean; to: number | null };
type Hold = { dir: 1 | -1; released: boolean };
/** The tape as the player holds it, and what its phase and graces read of it. */
function deck() {
  const ramp = rampSlot();
  const motion = motionSlot<W, { dir: 1 | -1; speeding: boolean }, { from: number; to: number }>();
  let holding: Hold | null = null;
  let knob: 'forward' | 'stopping' | 'backward' | 'returning' = 'forward';
  let shownRamp: 'in' | 'out' | null = null;
  let graces = NO_DEADLINES;
  const view = () => ({ committed: ramp.committed, rewinding: motion.rewinding, holding, plainHold: motion.plainHold, glide: motion.glide, shownRamp, knob });
  return {
    ramp,
    motion,
    hold: (h: Hold | null) => (holding = h),
    knob: (k: typeof knob) => (knob = k),
    shown: (s: typeof shownRamp) => (shownRamp = s),
    phase: () => phaseOf(view()),
    /** settleTape's deadlines at `now`, and those reached. */
    settle(now: number) {
      const free = ramp.committed === null && !motion.rewinding;
      graces = entered(graces, { shownRamp: shownRamp !== null && free, knobTurn: knob === 'stopping' || knob === 'returning', idleHold: !!holding && free, quietRest: null }, now);
      const reached = due(graces, now);
      for (const k of reached) graces = { ...graces, [k]: null };
      return reached;
    },
  };
}

test('a wind to a point: committed to playing as it begins, nothing of it left once it lands', () => {
  const t = deck();
  t.motion.rewinding = { brake: null, reverse: false, to: 42 };
  t.ramp.committed = false;
  assert.equal(t.phase(), 'winding');
  assert.deepEqual(t.settle(0), []);
  // It lands (endRewind): the wind gone, then its commitment.
  t.motion.rewinding = null;
  t.ramp.committed = null;
  assert.equal(t.phase(), 'idle');
  assert.deepEqual(t.settle(10_000), []);
});

test('a hold: pressed, winding, let go, braking to a stand, and over', () => {
  const t = deck();
  t.hold({ dir: 1, released: false });
  assert.equal(t.phase(), 'arming');
  t.motion.rewinding = { brake: null, reverse: false, to: null };
  t.ramp.committed = false;
  t.hold({ dir: 1, released: true });
  t.motion.rewinding = { brake: 1000, reverse: false, to: null };
  assert.equal(t.phase(), 'braking');
  t.motion.rewinding = null;
  t.ramp.committed = null;
  t.hold(null);
  assert.equal(t.phase(), 'idle');
  assert.deepEqual(t.settle(GRACE_MS.idleHold * 3), []);
});

test('backwards: the knob turned, the tape playing backwards, the turn back forward', () => {
  const t = deck();
  t.knob('stopping');
  assert.deepEqual(t.settle(0), []);
  t.knob('backward');
  t.motion.rewinding = { brake: null, reverse: true, to: null };
  t.ramp.committed = false;
  assert.equal(t.phase(), 'winding');
  assert.deepEqual(t.settle(GRACE_MS.knobTurn + 1), [], 'the turn ended in time');
  t.knob('returning');
  assert.deepEqual(t.settle(GRACE_MS.knobTurn + 2), []);
  assert.deepEqual(t.settle(2 * GRACE_MS.knobTurn + 3), ['knobTurn'], 'a turn standing past its grace ends forward');
});

test("a hold through a track's edge, or a track ending in a hold: the wind stops, the finger still down; its grace, then over", () => {
  const t = deck();
  t.hold({ dir: 1, released: false });
  t.motion.rewinding = { brake: null, reverse: false, to: null };
  t.ramp.committed = false;
  t.settle(100);
  // The edge (edges: stop), or the track's end: the wind ends without the hold.
  t.motion.rewinding = null;
  t.ramp.committed = null;
  assert.equal(t.phase(), 'arming');
  assert.deepEqual(t.settle(200), []);
  assert.deepEqual(t.settle(200 + GRACE_MS.idleHold), [], 'within its grace (a flight may yet arm)');
  assert.deepEqual(t.settle(201 + GRACE_MS.idleHold), ['idleHold']);
});

test('a seek in a wind keeps the wind (it goes on from there); a pause in a wind ends it and stops committed to pausing', () => {
  const t = deck();
  t.motion.rewinding = { brake: null, reverse: false, to: 30 };
  t.ramp.committed = false;
  // A seek: the wind's own (it moves where it goes); still winding.
  t.motion.rewinding = { brake: null, reverse: false, to: 60 };
  assert.equal(t.phase(), 'winding');
  // A pause (windTakeover): the wind ends, the stop is a ramp of its own, committed to pausing.
  t.motion.rewinding = null;
  t.ramp.committed = true;
  t.ramp.heldAt = 61;
  assert.equal(t.phase(), 'ramp');
  t.ramp.committed = null;
  assert.equal(t.ramp.heldAt, null);
});

test("the page's own player winds; the app's glides between its reports, and a wind here replaces its glide", () => {
  const t = deck();
  t.motion.glide = { from: 20, to: 18 };
  assert.equal(t.phase(), 'gliding');
  t.motion.glide = { from: 18, to: 16 };
  assert.equal(t.phase(), 'gliding');
  t.motion.rewinding = { brake: null, reverse: false, to: null };
  assert.equal(t.motion.glide, null);
  assert.equal(t.phase(), 'winding');
});

test('a ramp shown while nothing runs ends after its grace (the app has not said yet)', () => {
  const t = deck();
  t.shown('in');
  assert.deepEqual(t.settle(0), []);
  assert.deepEqual(t.settle(GRACE_MS.shownRamp + 1), ['shownRamp']);
  const u = deck();
  u.shown('in');
  u.ramp.committed = false;
  assert.deepEqual(u.settle(0), []);
  assert.deepEqual(u.settle(GRACE_MS.shownRamp * 5), [], 'a ramp running: shown as it is');
});

// Directions are pure (0.12.4; the owner, 2026-10-31): a hold never moves the knob's setting; the deck's L and R follow
// the way the tape goes. The two holds the owner met, step by step, through lib/player/knob's rules.
const knobRules = await import('../src/lib/player/knob.ts');

test('the knob at L, a forward hold and its let-go: the setting L throughout, backwards at 1× after', () => {
  const k = knobRules;
  const want: 'L' | 'R' = 'L';
  const seen: { want: string; way: 'L' | 'R' }[] = [];
  let way: 'L' | 'R' = 'L';
  // Playing backwards at 1×, then braked through a stop for the forward hold, wound forwards, let go to a stand,
  // backwards again (lib/localPlayer windHold with stopBackwards, letGo, arrive's backTo).
  assert.equal(k.holdInReverse(1), 'turn');
  for (const v of [-1, -0.4, 0, 2, 8, 3, 0, -0.5, -1]) seen.push({ want, way: (way = k.travelOf(v, way)) });
  assert.equal(k.letGoTo(1, true, want), 0);
  assert.equal(k.afterLetGo(1, true, want, 'play'), 'backwards');
  assert.ok(seen.every((s) => s.want === 'L'), 'the setting never moved');
  assert.deepEqual(seen.map((s) => s.way), ['L', 'L', 'L', 'R', 'R', 'R', 'R', 'L', 'L'], 'the deck said the way it went');
});

test('the knob at R, a back hold and its let-go: forwards at 1× after, the setting R throughout', () => {
  const k = knobRules;
  assert.equal(k.letGoTo(-1, false, 'R'), 0);
  assert.equal(k.afterLetGo(-1, false, 'R', 'play'), 'forwards');
  let way: 'L' | 'R' = 'R';
  const ways = [1, 0, -3, -6, -1, 0, 1].map((v) => (way = k.travelOf(v, way)));
  assert.deepEqual(ways, ['R', 'R', 'L', 'L', 'L', 'L', 'R']);
});

test("the knob at L, paused: a hold Next let go plays backwards, the knob's way (as the app does)", () => {
  const k = knobRules;
  // From a pause, not from playing backwards: what follows is as at R (here it plays on), its way the knob's.
  assert.equal(k.letGoTo(1, false, 'L'), 0, 'the forward hold brakes to a stand');
  assert.equal(k.afterLetGo(1, false, 'L', 'play'), 'backwards', 'then backwards at 1×');
  assert.equal(k.afterLetGo(1, false, 'L', 'pause'), 'rest', 'or resting there, as at R');
  assert.equal(k.afterLetGo(1, false, 'R', 'play'), 'forwards', 'at R: forwards, as before');
});

test('the knob at L, paused: a hold Previous let go plays on backwards at 1×', () => {
  const k = knobRules;
  assert.equal(k.letGoTo(-1, false, 'L'), 1, 'braked to 1× on its own way');
  assert.equal(k.afterLetGo(-1, false, 'L', 'play'), 'backwards');
  assert.equal(k.letGoTo(-1, false, 'R'), 0, 'at R: a stand, then forwards, as before');
  assert.equal(k.afterLetGo(-1, false, 'R', 'play'), 'forwards');
});

test("a skip in either setting: the new track the setting's way, the setting kept", () => {
  assert.equal(knobRules.skipWay('L'), 'backwards');
  assert.equal(knobRules.skipWay('R'), 'forwards');
});
