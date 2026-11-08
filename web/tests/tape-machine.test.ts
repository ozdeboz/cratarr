// The deck's machine as the page calls it in tape.wasm (lib/tape/deck: the rules crate's deck::next, a5's): what the
// page sends in and reads out says what the rules crate's own tests say, through the engine's numbers.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { machine } = await import('../src/lib/tape/deck.ts');
type Settings = import('../src/lib/tape/deck.ts').Settings;
type Here = import('../src/lib/tape/deck.ts').Here;
type Event = import('../src/lib/tape/deck.ts').Event;
type DeckWasm = import('../src/lib/tape/deck.ts').DeckWasm;
type TapeWasm = import('../src/lib/tape/head.ts').TapeWasm;

const bytes = readFileSync(new URL('../src/lib/wasm/tape.wasm', import.meta.url));
const x = (await WebAssembly.instantiate(bytes)).instance.exports as unknown as DeckWasm & TapeWasm;

/** Every motion winds, Start and stop on, edges stop, no repeat. */
const ON: Settings = { edges: 'stop', toEnd: true, toStart: true, seekBack: true, seekFwd: true, holdBack: true, holdFwd: true, startStop: true, skips: true, repeat: 'off' };
const MID: Here = { track: 1, tracks: 3, ms: 60_000, len: 120_000, more: false };

/** A deck playing forwards at `here` (spun up from a stand, or at once without Start and stop), and what it says to
 *  the events after. */
function playing(s: Settings, here: Here) {
  const m = machine(x, 'R');
  assert.ok(m);
  if (s.startStop) {
    assert.deepEqual(m.next(s, here, { kind: 'play' }), [{ do: 'spin-up', way: 'forwards' }]);
    assert.deepEqual(m.next(s, here, { kind: 'done' }), [{ do: 'play', way: 'forwards' }]);
  } else assert.deepEqual(m.next(s, here, { kind: 'play' }), [{ do: 'play', way: 'forwards' }]);
  assert.equal(m.tape(), 'playing');
  return { m, run: (e: Event, h: Here = here) => m.next(s, h, e) };
}

test('a turn brakes to a stand first, then spins up the new way', () => {
  const { m, run } = playing(ON, MID);
  assert.deepEqual(run({ kind: 'turn', knob: 'L' }), [{ do: 'let-go', toOne: false }]);
  assert.equal(m.knob(), 'L');
  assert.equal(m.tape(), 'braking-to-stand');
  assert.deepEqual(run({ kind: 'done' }), [{ do: 'stand' }, { do: 'spin-up', way: 'backwards' }]);
  assert.equal(m.way(), 'backwards');
  m.free();
});

test('Next during a pause\'s brake: the tape stands at the next track, still paused', () => {
  const { m, run } = playing(ON, MID);
  assert.deepEqual(run({ kind: 'pause' }), [{ do: 'let-go', toOne: false }]);
  assert.deepEqual(run({ kind: 'tap-next' }), []);
  assert.deepEqual(run({ kind: 'done' }), [{ do: 'stand' }, { do: 'go-to', track: 2, at: { at: 'start' } }]);
  assert.equal(m.tape(), 'standing');
  m.free();
});

test('Start and stop off: a pause stands at once; Next without its wind is a cut', () => {
  const { m, run } = playing({ ...ON, startStop: false, toEnd: false, skips: false }, MID);
  assert.deepEqual(run({ kind: 'tap-next' }), [{ do: 'go-to', track: 2, at: { at: 'start' } }, { do: 'play', way: 'forwards' }]);
  assert.deepEqual(run({ kind: 'pause' }), [{ do: 'stand' }]);
  m.free();
});

test('repeat all goes round the queue; repeat one plays the track again', () => {
  const last: Here = { ...MID, track: 2 };
  const all = playing({ ...ON, repeat: 'all' }, last);
  assert.deepEqual(all.run({ kind: 'track-ended' }), [{ do: 'go-to', track: 0, at: { at: 'start' } }, { do: 'play', way: 'forwards' }]);
  all.m.free();
  const one = playing({ ...ON, repeat: 'one' }, MID);
  assert.deepEqual(one.run({ kind: 'track-ended' }), [{ do: 'go-to', track: 1, at: { at: 'start' } }, { do: 'play', way: 'forwards' }]);
  one.m.free();
});

// The owner, 2026-11-04: Next with Autoplay's picks on their way winds to the end like any Next; straight on into the
// first pick if it is in by then, else the tape stands at the end and starts the pick (spun up) as soon as it comes.
const END: Here = { ...MID, track: 2, more: true };
const GROWN: Here = { ...END, tracks: 4 };

test('Autoplay: the last track played out, it waits at the end, and the pick starts spun up once it came', () => {
  const { m, run } = playing(ON, END);
  assert.deepEqual(run({ kind: 'track-ended' }), [{ do: 'stand' }]);
  assert.equal(m.tape(), 'awaiting-to-play');
  assert.deepEqual(run({ kind: 'grew' }, GROWN), [{ do: 'go-to', track: 3, at: { at: 'start' } }, { do: 'spin-up', way: 'forwards' }]);
  assert.equal(m.tape(), 'spinning-up');
  m.free();
});

test('Autoplay: Next winds to the end, flying, and straight on into the pick that came meanwhile', () => {
  const { m, run } = playing(ON, END);
  assert.deepEqual(run({ kind: 'tap-next' }), [{ do: 'tap', to: { at: 'end' }, way: 'forwards', fromStill: false, landsStill: false }]);
  assert.deepEqual(run({ kind: 'done' }, GROWN), [{ do: 'cross', track: 3, at: { at: 'start' } }, { do: 'play', way: 'forwards' }]);
  assert.equal(m.tape(), 'playing');
  m.free();
});

test('Autoplay: Next winds to the end, no pick yet: braked to a stand there, then on once it came', () => {
  const { m, run } = playing(ON, END);
  run({ kind: 'tap-next' });
  assert.deepEqual(run({ kind: 'done' }), [{ do: 'let-go', toOne: false }]);
  assert.deepEqual(run({ kind: 'done' }), [{ do: 'stand' }]);
  assert.equal(m.tape(), 'awaiting-to-play');
  assert.deepEqual(run({ kind: 'grew' }, GROWN), [{ do: 'go-to', track: 3, at: { at: 'start' } }, { do: 'spin-up', way: 'forwards' }]);
  m.free();
});

test('a hold flown from 1×, let go, plays on at 1× its way', () => {
  const { m, run } = playing(ON, MID);
  assert.deepEqual(run({ kind: 'hold-start', way: 'forwards' }), [{ do: 'hold', way: 'forwards', fromStill: false }]);
  assert.equal(m.tape(), 'held');
  assert.deepEqual(run({ kind: 'hold-end', way: 'forwards' }), [{ do: 'let-go', toOne: true }]);
  assert.deepEqual(run({ kind: 'done' }), [{ do: 'play', way: 'forwards' }]);
  m.free();
});

test('when a motion has done what it is for: a tap at its length, a spin-up at its attack, a hold never', () => {
  const tap = x.motion_tap(3000, 1000, 200, 200, 0, 1);
  assert.equal(x.motion_done_at(tap), 1000);
  const rise = x.motion_rise(1, 400);
  assert.equal(x.motion_done_at(rise), 400);
  const hold = x.motion_hold(2, 16, 500, 1);
  assert.equal(x.motion_done_at(hold), -1);
  x.motion_let_go(hold, 1000, 0, 300);
  assert.equal(x.motion_done_at(hold), x.motion_stands_at(hold));
  for (const h of [tap, rise, hold]) x.motion_free(h);
});

test('a queue entry chosen is a cut while playing; nothing for one outside the queue', () => {
  const { m, run } = playing(ON, MID);
  assert.deepEqual(run({ kind: 'jump', track: 0 }), [{ do: 'go-to', track: 0, at: { at: 'start' } }, { do: 'play', way: 'forwards' }]);
  assert.deepEqual(run({ kind: 'jump', track: 7 }), []);
  assert.equal(m.tape(), 'playing');
  m.free();
});

test('what the page shows as playing: a pause\'s brake not, a turn\'s brake yes', () => {
  const a = playing(ON, MID);
  assert.equal(a.m.plays(), true);
  a.run({ kind: 'pause' });
  assert.equal(a.m.plays(), false);
  a.m.free();
  const b = playing(ON, MID);
  b.run({ kind: 'turn', knob: 'L' });
  assert.equal(b.m.tape(), 'braking-to-stand');
  assert.equal(b.m.plays(), true);
  b.m.free();
});

test('Stop: at rest at once from anything, the knob untouched; standing, nothing more', () => {
  const { m, run } = playing(ON, MID);
  run({ kind: 'hold-start', way: 'forwards' });
  assert.deepEqual(run({ kind: 'stop' }), [{ do: 'stand' }]);
  assert.equal(m.tape(), 'standing');
  assert.equal(m.knob(), 'R');
  assert.deepEqual(run({ kind: 'stop' }), []);
  m.free();
});

test('a play during a pause\'s brake turns it back up there, to 1x', () => {
  const { m, run } = playing(ON, MID);
  run({ kind: 'pause' });
  assert.deepEqual(run({ kind: 'play' }), [{ do: 'let-go', toOne: true }]);
  assert.equal(m.tape(), 'braking-to-one');
  assert.equal(m.plays(), true);
  assert.deepEqual(run({ kind: 'done' }), [{ do: 'play', way: 'forwards' }]);
  m.free();
});
