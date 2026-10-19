// The signals on one scale (normal.ts, decision 0056 step 5): 1 = typical against this track, lately or absolute,
// fast or slow; the server's typical values and what is heard as it plays give the same scale.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MOST, newNormalState, signalsFor, typicalOf, type MusicNow } from '../src/lib/visuals/normal.ts';
import { B, BAND_SIGNALS, T, TRACK_SIGNALS } from '../src/lib/visuals/routes.ts';
import { ID } from '../src/lib/visuals/signal-ids.ts';
import type { Signals } from '../src/lib/visuals/signals.ts';
import type { Beats } from '../src/lib/visuals/beats.ts';

const BANDS = 6;
const near = (a: number, b: number, eps = 0.02) => assert.ok(Math.abs(a - b) <= eps, `${a} is not near ${b}`);

/** Signals with each band's level a series of `levels` (bytes, 20 a second), its hits of `hits` strengths. */
function signalsWith(levels: number[], hits: number[]): Signals {
  const data = new Uint8Array(levels.length * BANDS);
  levels.forEach((l, i) => data.fill(l, i * BANDS, (i + 1) * BANDS));
  const at = Float64Array.from(hits, (_, i) => i);
  const value = Float32Array.from(hits);
  return {
    edges: [],
    series: new Map([[ID.level, { rate: 20, channels: BANDS, scale: 0, data }]]),
    hits: new Map([[ID.hits, Array.from({ length: BANDS }, () => ({ at, value }))]]),
    marks: new Map(),
    perSecond: [],
    meanStrength: [],
  };
}

const frame = (key: string, level: number, hit = 0, typical: MusicNow['typical'] = null): MusicNow => {
  const bands = Array.from({ length: BANDS }, () => {
    const v = new Float32Array(BAND_SIGNALS.length);
    v[B.level] = level;
    v[B.hits] = hit;
    return v;
  });
  return { key, track: new Float32Array(TRACK_SIGNALS.length), bands, typical };
};

test("this track: a band's level is its median where it sounds, silence left out", () => {
  // Half the frames silent, the rest at 51 (0.2) and 102 (0.4) in equal measure, a few at 204 (0.8): median 0.4.
  const levels = [...Array(100).fill(0), ...Array(40).fill(51), ...Array(50).fill(102), ...Array(10).fill(204)];
  const typical = typicalOf(signalsWith(levels, [0.5, 0.25, 0.5]), null);
  near(typical.bands[2][B.level], 0.4);
  near(typical.bands[2][B.hits], 0.5);
  const st = newNormalState();
  const read = signalsFor(st, frame('a', 0.8, 0, typical), 0);
  near(read({ signal: 'level', band: 2 }), 2);
  near(read({ signal: 'level', band: 2, reference: 'absolute' }), 0.8);
  near(read({ signal: 'level' }), 2, 0.02);
  assert.equal(read({ signal: 'no such' }), 0);
  assert.equal(read({ signal: 'level', band: 9 }), 0);
});

test('heard as it plays: the same median, from what has been heard so far', () => {
  const st = newNormalState();
  // Before enough has been heard, a level reads as it is.
  near(signalsFor(st, frame('b', 0.3), 0)({ signal: 'level', band: 0 }), 0.3);
  let read = signalsFor(st, frame('b', 0.3), 16);
  for (let i = 0; i < 300; i++) read = signalsFor(st, frame('b', i % 2 ? 0.2 : 0.4), 32 + i * 16);
  // Heard: 0.2 and 0.4 in equal measure, one 0.3: the median about 0.3 (histogram steps of 0.01).
  near(signalsFor(st, frame('b', 0.6), 32 + 300 * 16)({ signal: 'level', band: 0 }), 0.6 / 0.3, 0.15);
});

test("a band's hits are typical at the top of a typical hit, heard or from the server", () => {
  const st = newNormalState();
  // Hits of 0.5, each ringing down over five frames.
  let t = 0;
  let read = signalsFor(st, frame('c', 0, 0), t);
  for (let h = 0; h < 40; h++)
    for (const light of [0.5, 0.4, 0.3, 0.2, 0.1, 0]) read = signalsFor(st, frame('c', 0.5, light), (t += 16));
  const top = signalsFor(st, frame('c', 0.5, 0.5), (t += 16));
  near(top({ signal: 'hits', band: 1 }), 1, 0.05);
});

test('lately: against its own mean of the last few seconds; slow follows', () => {
  const st = newNormalState();
  let read = signalsFor(st, frame('d', 0.2), 0);
  for (let i = 1; i <= 600; i++) read = signalsFor(st, frame('d', 0.2), i * 16);
  near(read({ signal: 'level', band: 0, reference: 'lately' }), 1);
  // A sudden rise: the fast form doubles at once, the slow one has not got there yet.
  read = signalsFor(st, frame('d', 0.4), 601 * 16);
  near(read({ signal: 'level', band: 0, reference: 'lately' }), 2, 0.05);
  const slow = read({ signal: 'level', band: 0, reference: 'absolute', speed: 'slow' });
  assert.ok(slow > 0.2 && slow < 0.4, `${slow}`);
});

test('the same at any frame rate', () => {
  const run = (step: number) => {
    const st = newNormalState();
    let read = signalsFor(st, frame('e', 0), 0);
    for (let t = step; t <= 2000; t += step) read = signalsFor(st, frame('e', t < 1000 ? 0.1 : 0.5), t);
    return read({ signal: 'level', band: 0, reference: 'absolute', speed: 'slow' });
  };
  near(run(1000 / 30), run(1000 / 120), 0.01);
});

test('beats, tempo and downbeats from the beats; the key only ever as it is', () => {
  const at = Float64Array.from({ length: 64 }, (_, i) => i * 0.5);
  const strength = Float32Array.from({ length: 64 }, (_, i) => (i % 4 === 0 ? 1 : 0.5));
  const beats = { at, strength, typical: new Float32Array(64).fill(0.5) } as unknown as Beats;
  const bar: Signals = {
    ...signalsWith([1], []),
    series: new Map([[ID.bar, { rate: 0, channels: 1, scale: 255, data: Uint8Array.from({ length: 64 }, (_, i) => i % 4) }]]),
  };
  const typical = typicalOf(bar, beats);
  near(typical.track[T.beat], 0.35 + 0.65 * 0.5);
  near(typical.track[T.tempo], 1);
  near(typical.track[T.downbeat], 1);
  assert.equal(typicalOf(bar, beats), typical, 'worked out once');
  const f = frame('f', 0, 0, typical);
  f.track[T.tempo] = 1;
  f.track[T.key] = 0.25;
  const read = signalsFor(newNormalState(), f, 0);
  near(read({ signal: 'tempo' }), 1);
  near(read({ signal: 'key', reference: 'track' }), 0.25);
  near(read({ signal: 'loudness' }), 0);
});

test('never past MOST, nor anything but a number', () => {
  const st = newNormalState();
  const typical = typicalOf(signalsWith(Array(20).fill(3), []), null);
  const f = frame('g', Number.NaN, 0, typical);
  assert.equal(signalsFor(st, f, 0)({ signal: 'level', band: 0 }), 0);
  near(signalsFor(st, frame('g', 1, 0, typical), 16)({ signal: 'level', band: 0 }), MOST, 0);
});
