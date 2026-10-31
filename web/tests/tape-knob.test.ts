// The direction knob and the holds against it (lib/player/knob), over the sequences the owner met on 2026-10-30 (the
// ThinkPad's playback log): a hold back whose stream came late, Previous while playing backwards, holding back while
// playing backwards, holding forward there.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { afterLetGo, holdInReverse, knobFor, letGoTo, notSounding, skipWay, travelOf } = await import('../src/lib/player/knob.ts');

type Knob = { want: 'L' | 'R'; state: 'forward' | 'stopping' | 'backward' | 'returning' };
const late = { point: false, handed: false, held: true, reverse: false, retried: false, refused: false };

test('a hold back whose stream comes late: asked again, then resting where it stood, never forwards', () => {
  // 15:20:20–15:21:49: nine holds, the stream not sounding in 1.5 s, each played on forwards with the finger down.
  assert.equal(notSounding(late), 'retry');
  // A hold forward from a pause the same (it waited there; it does not start playing by itself).
  assert.equal(notSounding({ ...late, retried: true }), 'rest');
  // A play the browser refused is not asked again.
  assert.equal(notSounding({ ...late, refused: true }), 'rest');
});

test('let go before it sounded, a tap or a seek, a flight or a handed brake: plainly, as before', () => {
  assert.equal(notSounding({ ...late, held: false }), 'plain', 'a hold let go plays on where it was');
  assert.equal(notSounding({ ...late, point: true }), 'plain', 'a tap or a seek goes there');
  assert.equal(notSounding({ ...late, handed: true }), 'plain', 'a flight from play, a let-go brake');
});

test("the knob's backwards whose stream comes late: asked again, then resting, the knob still at L", () => {
  const knob: Knob = { want: 'L', state: 'backward' };
  assert.equal(notSounding({ ...late, held: false, reverse: true }), 'retry');
  assert.equal(notSounding({ ...late, held: false, reverse: true, retried: true }), 'rest');
  // Rested: nothing plays backwards; the way stands forwards, the setting as the user left it (directions are pure).
  assert.deepEqual(knobFor(knob, null), { want: 'L', state: 'forward' });
});

test('a skip plays the way the knob is set, the knob left as it is (directions are pure, 0.12.4)', () => {
  // 0.10.1 put the knob at R on a skip; the owner (2026-10-31): only the user's turn changes it.
  assert.equal(skipWay('L'), 'backwards', 'at L: the new track backwards from its end');
  assert.equal(skipWay('R'), 'forwards', 'at R: from its start');
  let knob: Knob = { want: 'L', state: 'backward' };
  assert.equal(knobFor(knob, { reverse: true, backTo: false }), null, 'playing backwards: the knob stands');
  knob = { ...knob, state: 'forward' };
  assert.equal(knob.want, 'L', 'the skip leaves the setting');
});

test('the way left backwards with nothing playing backwards: put right, the setting kept', () => {
  assert.deepEqual(knobFor({ want: 'L', state: 'backward' }, { reverse: false, backTo: false }), { want: 'L', state: 'forward' });
  assert.equal(knobFor({ want: 'L', state: 'forward' }, null), null, 'paused at L: the next play goes backwards, as it should');
  assert.equal(knobFor({ want: 'L', state: 'stopping' }, null), null, 'mid-turn: the turn finishes');
});

test('holding back while playing backwards: flown on, faster and faster; let go, backwards again at 1×', () => {
  assert.equal(holdInReverse(-1), 'fly');
  let knob: Knob = { want: 'L', state: 'backward' };
  // While the hold winds (not the 1× backwards play) the knob stays backwards.
  assert.equal(knobFor(knob, { reverse: false, backTo: true }), null);
  // Let go: braked to 1× backwards, and it plays on that way.
  assert.equal(letGoTo(-1, true, knob.want), 1);
  assert.equal(afterLetGo(-1, true, knob.want, 'play'), 'backwards');
  // A pause pressed during the brake: it rests there, the knob still at L.
  assert.equal(afterLetGo(-1, true, knob.want, 'pause'), 'rest');
  // The knob turned to R while held: let go, it brakes to a stand and plays on forwards.
  knob = { want: 'R', state: 'backward' };
  assert.equal(letGoTo(-1, true, knob.want), 0);
  assert.equal(afterLetGo(-1, true, knob.want, 'play'), 'forwards');
});

test('the knob at L, holding forward: braked through a stop, winding forwards; let go, a stand, then backwards at 1×', () => {
  // The owner (2026-10-31): "holding forward in order to wind to the right side switches the direction toggle on the
  // tape deck and doesn't switch it back". The setting stays at L throughout; the hold returns to it.
  assert.equal(holdInReverse(1), 'turn');
  const knob: Knob = { want: 'L', state: 'forward' };
  assert.equal(letGoTo(1, true, knob.want), 0, 'braked to a stand (through zero)');
  assert.equal(afterLetGo(1, true, knob.want, 'play'), 'backwards', 'then backwards at 1×, the knob still at L');
  assert.equal(afterLetGo(1, true, knob.want, 'pause'), 'rest');
  // The knob turned to R by the user during the hold: let go at 1× forwards.
  assert.equal(letGoTo(1, true, 'R'), 1);
  assert.equal(afterLetGo(1, true, 'R', 'play'), 'forwards');
});

test('holding forward with the knob at R: let go at 1× forwards, as before', () => {
  assert.equal(letGoTo(1, false, 'R'), 1);
  assert.equal(afterLetGo(1, false, 'R', 'play'), 'forwards');
});

test("the deck's L and R are the way the tape goes, frame by frame; standing, the last way kept", () => {
  // A hold back from forwards play (the knob at R): R, slowing, a stand, then L as it winds back, R again after.
  const speeds = [1, 0.5, 0.01, -0.5, -4, -0.3, 0, 0.4, 1];
  const ways: ('L' | 'R')[] = [];
  let last: 'L' | 'R' = 'R';
  for (const v of speeds) ways.push((last = travelOf(v, last)));
  assert.deepEqual(ways, ['R', 'R', 'R', 'L', 'L', 'L', 'L', 'R', 'R']);
  // Each frame's way is its speed's sign where it moves.
  speeds.forEach((v, i) => Math.abs(v) > 0.02 && assert.equal(ways[i], v < 0 ? 'L' : 'R'));
});

test('a hold back from forwards play is as before: braked to a stand, then forwards', () => {
  assert.equal(letGoTo(-1, false, 'R'), 0);
  assert.equal(afterLetGo(-1, false, 'R', 'play'), 'forwards');
  // The knob at L, the hold from a pause (0.12.4: it ended forwards; the app ended backwards, by the owner's rule):
  // the knob's way, backwards at 1×.
  assert.equal(letGoTo(-1, false, 'L'), 1);
  assert.equal(afterLetGo(-1, false, 'L', 'play'), 'backwards');
});
