// The routes with curves, and the levels as the most they get (routes.ts, curves.ts): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { A, ASPECTS, B, BAND_SIGNALS, compile, give, given, routeKey, T, TRACK_SIGNALS } from '../src/lib/visuals/routes.ts';
import { heightAt, isStraight, straight, tableOf, valueOf, type Curve } from '../src/lib/visuals/curves.ts';

const BANDS = 6;
const NA = ASPECTS.length;
const close = (a: number, b: number, eps = 1e-5) => Math.abs(a - b) < eps;

/** The routes as they were added up before curves (64): the reference the straight routes must still match. */
function oldGive(routes: Record<string, number>, track: Float32Array, bands: Float32Array[], envelope: number) {
  const whole = new Float32Array(NA);
  const band = Array.from({ length: BANDS }, () => new Float32Array(NA));
  for (const [key, amount] of Object.entries(routes)) {
    const [signal, b, aspect] = key.split('|');
    const a = A[aspect as keyof typeof A];
    if (b === '-') {
      const s = T[signal as keyof typeof T];
      whole[a] += amount * track[s] * (s === T.loudness ? 1 : envelope);
    } else band[Number(b)][a] += amount * bands[Number(b)][B[signal as keyof typeof B]] * envelope;
  }
  return { whole, band };
}

const signalsNow = (seed: number) => {
  let x = seed;
  const rnd = () => ((x = (x * 1103515245 + 12345) % 2147483648) / 2147483648);
  return {
    track: Float32Array.from(TRACK_SIGNALS, () => rnd()),
    bands: Array.from({ length: BANDS }, () => Float32Array.from(BAND_SIGNALS, () => rnd())),
  };
};

test('straight routes give exactly what they gave before curves (the swings)', () => {
  const routes = {
    [routeKey('loudness', null, 'size')]: 1,
    [routeKey('beat', null, 'brightness')]: 0.2,
    [routeKey('section', null, 'look')]: -0.5,
    [routeKey('level', 2, 'shape')]: 1,
    [routeKey('hits', 4, 'lights')]: 0.7,
  };
  const m = compile(routes, BANDS);
  for (let seed = 1; seed < 20; seed++) {
    const { track, bands } = signalsNow(seed);
    const e = (seed % 10) / 10;
    const got = give(m, track, bands, e, given(BANDS));
    const want = oldGive(routes, track, bands, e);
    for (const a of ['size', 'brightness', 'look'] as const) assert.ok(close(got.whole[A[a]], want.whole[A[a]]), `${a} ${got.whole[A[a]]} vs ${want.whole[A[a]]}`);
    assert.ok(close(got.band[2][A.shape], want.band[2][A.shape]) && close(got.band[4][A.lights], want.band[4][A.lights]));
  }
});

test('a level is the most its slider gives: straight routes as conditions, never past it nor under nothing', () => {
  const s = 0.3;
  const e = 0.8;
  const one = (amount: number, signal: number) => {
    const m = compile({ [routeKey('swell', null, 'fluid')]: amount }, BANDS);
    const track = new Float32Array(TRACK_SIGNALS.length);
    track[T.swell] = signal;
    return give(m, track, [], e, given(BANDS)).level[A.fluid];
  };
  assert.ok(close(one(0.6, s), 1 - e * 0.6 * (1 - s)), 'positive: 1 − e·a·(1 − s)');
  assert.ok(close(one(-0.6, s), 1 - e * 0.6 * s), 'negative: 1 − e·a·s');
  assert.equal(one(0.6, 1), 1, 'positive at a full signal: all of it');
  assert.equal(one(-0.6, 0), 1, 'negative at no signal: all of it');
  for (const a of [-1, -0.4, 0.4, 1]) for (const v of [0, 0.5, 1]) assert.ok(one(a, v) >= 0 && one(a, v) <= 1);
  // Quiet: every route lets all through.
  const m = compile({ [routeKey('swell', null, 'fluid')]: 1 }, BANDS);
  assert.equal(give(m, new Float32Array(TRACK_SIGNALS.length), [], 0, given(BANDS)).level[A.fluid], 1);
});

test('a level’s routes multiply, its bands’ by their geometric mean', () => {
  const track = new Float32Array(TRACK_SIGNALS.length);
  track[T.swell] = 0.5;
  track[T.section] = 0.25;
  const m = compile({ [routeKey('swell', null, 'smoothing')]: 1, [routeKey('section', null, 'smoothing')]: 1 }, BANDS);
  assert.ok(close(give(m, track, [], 1, given(BANDS)).level[A.smoothing], 0.5 * 0.25));
  // One band's route into a level: its factor to the sixth root, as the swings take the bands' mean.
  const bands = Array.from({ length: BANDS }, () => new Float32Array(BAND_SIGNALS.length));
  const mb = compile({ [routeKey('flux', 0, 'haze')]: 1 }, BANDS);
  assert.ok(close(give(mb, track, bands, 1, given(BANDS)).level[A.haze], 0 ** (1 / BANDS)));
  bands[0][B.flux] = 0.5;
  assert.ok(close(give(mb, track, bands, 1, given(BANDS)).level[A.haze], 0.5 ** (1 / BANDS)));
});

test('a curved route gives its curve’s height, and a level its curve’s pull below its peak', () => {
  // Rising fast, then flat: a bend no straight line has.
  const c: Curve = { h1: [0.1, 0.9], h2: [0.3, 1], h: 1 };
  assert.ok(!isStraight(c));
  const { table, peak } = tableOf(c);
  assert.ok(close(peak, 1, 1e-3) && close(heightAt(table, 0), 0) && close(heightAt(table, 1), 1, 1e-3));
  assert.ok(heightAt(table, 0.3) > 0.85, 'high early');
  const m = compile({ [routeKey('beat', null, 'size')]: c, [routeKey('beat', null, 'haze')]: c }, BANDS);
  const track = new Float32Array(TRACK_SIGNALS.length);
  track[T.beat] = 0.3;
  const g = give(m, track, [], 1, given(BANDS));
  assert.ok(close(g.whole[A.size], heightAt(table, 0.3), 1e-5));
  assert.ok(close(g.level[A.haze], 1 - (peak - heightAt(table, 0.3)), 1e-5));
});

test('a crossing curve reaches all of its level at its top, and its height goes below 0 where it dips', () => {
  const c: Curve = { h1: [0.3, -0.8], h2: [0.6, 1], h: -0.2 };
  const { table, peak } = tableOf(c);
  assert.ok(peak > 0 && Math.min(...table) < 0);
  let top = 0;
  for (let k = 0; k < table.length; k++) if (table[k] > table[top]) top = k;
  const m = compile({ [routeKey('beat', null, 'fluid')]: c }, BANDS);
  const track = new Float32Array(TRACK_SIGNALS.length);
  track[T.beat] = top / (table.length - 1);
  assert.ok(close(give(m, track, [], 1, given(BANDS)).level[A.fluid], 1, 1e-2));
});

test('a straight curve is kept as its number, a bent one as itself; routes at 0 are passed over', () => {
  assert.equal(valueOf(straight(0.4)), 0.4);
  const bent: Curve = { h1: [0.2, 0.5], h2: [0.5, 0.5], h: 0.4 };
  assert.deepEqual(valueOf(bent), bent);
  const m = compile({ [routeKey('beat', null, 'size')]: 0, [routeKey('loudness', null, 'size')]: 1 }, BANDS);
  assert.equal(m.trackOn.length, 1);
});

test('a route that walks gives its walk factor’s share of itself, and a level’s pull as much less', () => {
  const walking: Curve = { ...straight(0.8), walks: true };
  assert.deepEqual(valueOf(walking), walking, 'kept as a curve: a straight one that walks stays one');
  const m = compile({ [routeKey('beat', null, 'size')]: walking, [routeKey('swell', null, 'fluid')]: { ...straight(0.6), walks: true }, [routeKey('loudness', null, 'size')]: 1 }, BANDS);
  assert.deepEqual(m.walks, [routeKey('beat', null, 'size'), routeKey('swell', null, 'fluid')]);
  const track = new Float32Array(TRACK_SIGNALS.length);
  track[T.beat] = 0.5;
  track[T.swell] = 0.25;
  track[T.loudness] = 1;
  const at = (w: number[]) => give(m, track, [], 1, given(BANDS), Float32Array.from(w));
  // As set: 0.8 × 0.5 from the beat, 1 from the loudness; the fluid's factor 1 − 0.6 × 0.75.
  assert.ok(close(at([1, 1]).whole[A.size], 1.4) && close(at([1, 1]).level[A.fluid], 0.55));
  // Walked to 0.7: the beat's part 0.7 of itself; the fluid's pull 0.7 of itself.
  assert.ok(close(at([0.7, 0.7]).whole[A.size], 1 + 0.4 * 0.7));
  assert.ok(close(at([0.7, 0.7]).level[A.fluid], 1 - 0.7 * 0.45));
  // Without walk factors, as set.
  assert.ok(close(give(m, track, [], 1, given(BANDS)).whole[A.size], 1.4));
});

test('the look’s columns (65.1): an Amount is a level, a control a swing, a discrete setting a moment', () => {
  const track = new Float32Array(TRACK_SIGNALS.length);
  const bands = Array.from({ length: BANDS }, () => new Float32Array(BAND_SIGNALS.length));
  track[T.beat] = 0.25;
  const m = compile({ [routeKey('beat', null, 'fx1Amount')]: 1, [routeKey('beat', null, 'fx2K1')]: 0.8, [routeKey('beat', null, 'final')]: 1 }, BANDS);
  const g = give(m, track, bands, 1, given(BANDS));
  // The Amount: a condition for reaching its slider (a quarter of the beat, three quarters held back), not added.
  assert.ok(close(g.level[A.fx1Amount], 0.25), `${g.level[A.fx1Amount]}`);
  assert.equal(g.whole[A.fx1Amount], 0);
  // The control: what its route gives, added to where it is set.
  assert.ok(close(g.whole[A.fx2K1], 0.2), `${g.whole[A.fx2K1]}`);
  assert.equal(g.level[A.fx2K1], 1);
  // The Lens: what its route gives, for its moment.
  assert.ok(close(g.whole[A.final], 0.25));
});
