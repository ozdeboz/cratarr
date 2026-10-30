// The signals on one scale (normal.ts, decision 0056 step 5): 1 = typical against this track, lately or absolute,
// fast or slow; the server's typical values and what is heard as it plays give the same scale.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FLOOR, MOST, WARM_S, newNormalState, signalsFor, typicalOf, type MusicNow } from '../src/lib/visuals/normal.ts';
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

const frame = (key: string, level: number, hit = 0, typical: MusicNow['typical'] = null, playing = true): MusicNow => {
  const bands = Array.from({ length: BANDS }, () => {
    const v = new Float32Array(BAND_SIGNALS.length);
    v[B.level] = level;
    v[B.hits] = hit;
    return v;
  });
  return { key, playing, track: new Float32Array(TRACK_SIGNALS.length), bands, typical };
};

/** Frames of `level` every `step` ms from `from` to `to`; the last reader. */
function run(st: ReturnType<typeof newNormalState>, key: string, level: (t: number) => number, from: number, to: number, step = 16, playing = true) {
  let read = signalsFor(st, frame(key, level(from), 0, null, playing), from);
  for (let t = from + step; t <= to; t += step) read = signalsFor(st, frame(key, level(t), 0, null, playing), t);
  return read;
}

test("this track: a band's level is its median where it sounds, silence left out", () => {
  // Half the frames silent; of the rest 40 at 51 (0.2), 50 at 102 (0.4), 10 at 204 (0.8): the median 0.4.
  const levels = [...Array(100).fill(0), ...Array(40).fill(51), ...Array(50).fill(102), ...Array(10).fill(204)];
  const typical = typicalOf(signalsWith(levels, [0.5, 0.25, 0.5]), null);
  near(typical.bands[2][B.level], 0.4);
  near(typical.bands[2][B.hits], 0.5);
  const read = signalsFor(newNormalState(), frame('a', 0.8, 0, typical), 0);
  near(read({ signal: 'level', band: 2 }), 2);
  near(read({ signal: 'level', band: 2, reference: 'absolute' }), 0.8);
  near(read({ signal: 'level' }), 2);
  assert.equal(read({ signal: 'no such' }), 0);
  assert.equal(read({ signal: 'level', band: 9 }), 0);
});

test('a typical value is never under FLOOR: a mostly silent series does not throw every blip to the top', () => {
  const typical = typicalOf(signalsWith(Array(40).fill(1), []), null);
  const read = signalsFor(newNormalState(), frame('n', 0.05, 0, typical), 0);
  near(read({ signal: 'level', band: 0 }), 0.05 / FLOOR, 0.01);
  near(signalsFor(newNormalState(), frame('n2', 1, 0, typical), 0)({ signal: 'level', band: 0 }), MOST, 0);
});

test('heard as it plays: the median by time, crossed in from the value as it is over the first seconds', () => {
  const st = newNormalState();
  // At first, as it is.
  near(signalsFor(st, frame('b', 0.3), 0)({ signal: 'level', band: 0 }), 0.3);
  // A second of 0.2 then 0.6 alternating every frame: the median by time between them (byte steps).
  let read = run(st, 'b', (t) => (Math.round(t / 16) % 2 ? 0.2 : 0.6), 16, 1000);
  const early = read({ signal: 'level', band: 0 });
  // Warm: after WARM_S seconds of sound the ratio alone.
  run(st, 'b', (t) => (Math.round(t / 16) % 2 ? 0.2 : 0.6), 1016, 1000 + WARM_S * 1000 + 100);
  const typ = 0.6 / signalsFor(st, frame('b', 0.6), 1000 + WARM_S * 1000 + 108)({ signal: 'level', band: 0 });
  assert.ok(typ >= 0.19 && typ <= 0.61, `${typ}`);
  // Never a jump: one more frame moves the reading by little.
  const a = signalsFor(st, frame('b', 0.4), 1000 + WARM_S * 1000 + 116)({ signal: 'level', band: 0 });
  const b = signalsFor(st, frame('b', 0.4), 1000 + WARM_S * 1000 + 132)({ signal: 'level', band: 0 });
  near(a, b, 0.02);
  assert.ok(Number.isFinite(early));
});

test("a band's hits are typical at the top of a typical hit", () => {
  const st = newNormalState();
  let t = 0;
  signalsFor(st, frame('c', 0, 0), t);
  for (let h = 0; h < 40; h++) for (const light of [0.5, 0.4, 0.3, 0.2, 0.1, 0]) signalsFor(st, frame('c', 0.5, light), (t += 16));
  near(signalsFor(st, frame('c', 0.5, 0.5), (t += 16))({ signal: 'hits', band: 1 }), 1, 0.05);
});

test('lately: against its own mean of the last seconds, and a return from silence is not many times it', () => {
  const st = newNormalState();
  let read = run(st, 'd', () => 0.2, 0, 10_000);
  near(read({ signal: 'level', band: 0, reference: 'lately' }), 1);
  read = signalsFor(st, frame('d', 0.4), 10_016);
  near(read({ signal: 'level', band: 0, reference: 'lately' }), 2, 0.05);
  // From silence: the first sound reads about 1, not the cap.
  const quiet = newNormalState();
  run(quiet, 'q', () => 0, 0, 3000);
  near(signalsFor(quiet, frame('q', 0.3), 3016)({ signal: 'level', band: 0, reference: 'lately' }), 1, 0.05);
});

test('slow: up over SLOW_UP_S, the same at any frame rate', () => {
  const slowAt = (step: number) => {
    const st = newNormalState();
    const read = run(st, 'e', (t) => (t < 1000 ? 0.1 : 0.5), 0, 1100, step);
    return read({ signal: 'level', band: 0, reference: 'absolute', speed: 'slow' });
  };
  const a = slowAt(1000 / 30);
  const b = slowAt(1000 / 120);
  assert.ok(a > 0.2 && a < 0.5, `${a}`);
  near(a, b, 0.03);
});

test('paused: nothing is heard and nothing moves on', () => {
  const st = newNormalState();
  run(st, 'p', () => 0.2, 0, 2000);
  const before = signalsFor(st, frame('p', 0.2), 2016)({ signal: 'level', band: 0, reference: 'lately' });
  run(st, 'p', () => 0.9, 2032, 60_000, 16, false);
  const after = signalsFor(st, frame('p', 0.2), 60_016)({ signal: 'level', band: 0, reference: 'lately' });
  near(after, before, 0.02);
});

test('beats, tempo and downbeats from the beats; the key only ever as it is; a new track starts afresh', () => {
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
  const st = newNormalState();
  const read = signalsFor(st, f, 0);
  near(read({ signal: 'tempo' }), 1);
  near(read({ signal: 'key', reference: 'track' }), 0.25);
  near(read({ signal: 'loudness' }), 0);
  // Another track: what was heard of the last is gone.
  run(st, 'f', () => 0.8, 16, 8000);
  near(signalsFor(st, frame('g', 0.3), 8016)({ signal: 'level', band: 0 }), 0.3);
});

test('never past MOST, nor anything but a number', () => {
  const st = newNormalState();
  const typical = typicalOf(signalsWith(Array(20).fill(60), []), null);
  for (const bad of [Number.NaN, Number.POSITIVE_INFINITY, -1]) assert.equal(signalsFor(st, frame('h', bad, 0, typical), 0)({ signal: 'level', band: 0 }) >= 0, true);
  near(signalsFor(newNormalState(), frame('h2', 1, 0, typical), 0)({ signal: 'level', band: 0 }), 1 / (60 / 255), 0.05);
  const none = { key: 'z', playing: true, track: new Float32Array(TRACK_SIGNALS.length), bands: null, typical: null };
  assert.equal(signalsFor(newNormalState(), none, 0)({ signal: 'level', band: 0 }), 0);
});
