// The direction knob and the holds against it (lib/player/knob), over the sequences the owner met on 2026-10-30 (the
// ThinkPad's playback log): a hold back whose stream came late, Previous while playing backwards, holding back while
// playing backwards, holding forward there.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { afterLetGo, holdInReverse, knobFor, letGoTo, notSounding, SKIPPED } = await import('../src/lib/player/knob.ts');

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

test("the knob's backwards whose stream comes late: asked again, then resting, the knob at R", () => {
  const knob: Knob = { want: 'L', state: 'backward' };
  assert.equal(notSounding({ ...late, held: false, reverse: true }), 'retry');
  assert.equal(notSounding({ ...late, held: false, reverse: true, retried: true }), 'rest');
  // Rested: nothing plays backwards, so the knob is put back with it.
  assert.deepEqual(knobFor(knob, null), SKIPPED);
});

test('the knob at L, Previous while playing backwards in the track before: forwards from its start, the knob at R', () => {
  // 14:49:04: the knob turned L while playing; 14:49:11.7 the backwards play crossed into track 333; 14:49:12.2
  // Previous (333.9 s in, past the restart point): track 333 played from 0 forwards, the knob still at L.
  let knob: Knob = { want: 'L', state: 'backward' };
  assert.equal(knobFor(knob, { reverse: true, backTo: false }), null, 'playing backwards: the knob stands');
  // windTakeover('previous'): the skip puts the knob at R as it plays the track from its start.
  knob = { ...SKIPPED };
  assert.deepEqual(knob, { want: 'R', state: 'forward' });
  assert.equal(knobFor(knob, null), null, 'nothing left to put right');
});

test('the knob left backwards with nothing playing backwards: put right, want and state both', () => {
  // The old check put the state forward and left the knob showing L (the next play went backwards).
  assert.deepEqual(knobFor({ want: 'L', state: 'backward' }, { reverse: false, backTo: false }), SKIPPED);
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

test('holding forward while playing backwards: braked to a stop, the knob to R, winding forwards; let go at 1×', () => {
  assert.equal(holdInReverse(1), 'turn');
  assert.equal(letGoTo(1, false, 'R'), 1);
  assert.equal(afterLetGo(1, false, 'R', 'play'), 'forwards');
});

test('a hold back from forwards play is as before: braked to a stand, then forwards', () => {
  assert.equal(letGoTo(-1, false, 'R'), 0);
  assert.equal(afterLetGo(-1, false, 'R', 'play'), 'forwards');
  // The knob at L but the hold not flown from backwards play (paused at L): a stand, as any hold back.
  assert.equal(letGoTo(-1, false, 'L'), 0);
});
