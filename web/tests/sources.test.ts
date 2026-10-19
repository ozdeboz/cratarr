// The Shapes and Waves sources (sources/, decision 0056 step 4): their knobs' tables, nothing drawn at neutral, the
// polygons of fractional sides, the wave's three shapes mixed and closed. The GL is a stand-in that counts the draws.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SOURCE_TYPES, createSource } from '../src/lib/visuals/sources/index.ts';
import { defaults, neutrals } from '../src/lib/visuals/sources/knob.ts';
import { SHAPES } from '../src/lib/visuals/sources/shapes.ts';
import { WAVES } from '../src/lib/visuals/sources/waves.ts';
import {
  MOST_SHAPES, SEGMENT_FLOATS, SHAPE_FLOATS, TAU, WAVE_POINTS, closeSeam, morphEdge, outlineRadius, polygonEdge, resample,
  shapeLayout, synthWave, waveSegments,
} from '../src/lib/visuals/sources/geometry.ts';
import type { FrameInfo } from '../src/lib/visuals/patch.ts';

const close = (a: number, b: number, eps = 1e-6) => Math.abs(a - b) < eps;

/** A WebGL2 context that does nothing and counts the vertices drawn. */
function fakeGL() {
  const drawn: number[] = [];
  const gl = new Proxy({} as Record<string | symbol, unknown>, {
    get(_, p) {
      if (p === 'drawArrays') return (_m: number, _f: number, n: number) => drawn.push(n);
      if (typeof p === 'string' && p === p.toUpperCase()) return 0;
      return () => ({});
    },
  });
  return { gl: gl as unknown as WebGL2RenderingContext, drawn };
}
const frame = (waveform: Float32Array | null = null): FrameInfo => ({ dt: 1 / 60, time: 3, beat: null, signals: () => 1, waveform });

test('every knob continuous, its neutral and default in its range; the levels’ neutral their least', () => {
  for (const t of SOURCE_TYPES) {
    assert.equal(t.place, 'source');
    assert.equal(new Set(t.knobs.map((k) => k.id)).size, t.knobs.length, `${t.type}: ids repeat`);
    for (const k of t.knobs) {
      assert.ok(k.min < k.max, `${t.type}.${k.id}`);
      assert.ok(k.min <= k.neutral && k.neutral <= k.max, `${t.type}.${k.id} neutral`);
      assert.ok(k.min <= k.default && k.default <= k.max, `${t.type}.${k.id} default`);
      assert.notEqual(k.kind, 'step', `${t.type}.${k.id} is a step`);
      if (k.kind === 'level') assert.equal(k.neutral, k.min);
    }
  }
});

test('at neutral nothing is drawn; at the defaults something is', () => {
  for (const t of SOURCE_TYPES) {
    const { gl, drawn } = fakeGL();
    const s = createSource(t.type, gl)!;
    s.draw(gl, null, null, 800, 450, neutrals(t), frame());
    assert.deepEqual(drawn, [], `${t.type} drew at neutral`);
    // Each level knob alone at its neutral is enough.
    for (const k of t.knobs.filter((k) => k.kind === 'level')) {
      s.draw(gl, null, null, 800, 450, { ...defaults(t), [k.id]: k.neutral }, frame());
      assert.deepEqual(drawn, [], `${t.type} drew with ${k.id} at neutral`);
    }
    s.draw(gl, null, null, 800, 450, defaults(t), frame());
    assert.equal(drawn.length, 1, `${t.type} drew nothing at its defaults`);
  }
  assert.equal(createSource('flame', fakeGL().gl), null);
});

test('Waves draws the sound when there is some, else the bands; a segment a sample', () => {
  const { gl, drawn } = fakeGL();
  const w = createSource('waves', gl)!;
  const sound = new Float32Array(512).map((_, i) => Math.sin(i / 9));
  w.draw(gl, null, null, 640, 640, defaults(WAVES), frame(sound));
  w.draw(gl, null, null, 640, 640, defaults(WAVES), frame(null));
  assert.deepEqual(drawn, [WAVE_POINTS * 6, WAVE_POINTS * 6]);
});

test('a whole polygon: its corners at the radius, its sides’ middles at the radius × cos(π/n)', () => {
  for (const n of [3, 4, 5, 8]) {
    for (let c = 0; c < n; c++) assert.ok(close(outlineRadius((TAU * c) / n, n, 1), 1), `${n}: corner ${c}`);
    assert.ok(close(outlineRadius(Math.PI / n, n, 1), Math.cos(Math.PI / n)), `${n}: a side's middle`);
    assert.ok(close(polygonEdge(0.5, 0.1, n, 1), morphEdge(0.5, 0.1, n, 1)));
  }
});

test('fractional sides: within the corners’ circle, on the mixed edge’s zero, continuous, near a circle at many', () => {
  for (let sides = 3; sides <= 48; sides += 0.37)
    for (let t = 0; t < TAU; t += 0.05) {
      const r = outlineRadius(t, sides, 1);
      assert.ok(r <= 1 + 1e-9 && r > 0.49, `sides ${sides}, θ ${t}: ${r}`);
      assert.ok(Math.abs(morphEdge(r, t, sides, 1)) < 1e-9);
    }
  for (const n of [4, 5, 11])
    for (let t = 0; t < TAU; t += 0.1) assert.ok(close(outlineRadius(t, n - 1e-7, 1), outlineRadius(t, n, 1), 1e-5), `jump at ${n}`);
  // Half way from a square to a pentagon lies between them, at the square's side middle.
  const mid = outlineRadius(Math.PI / 4, 4.5, 1);
  assert.ok(mid > outlineRadius(Math.PI / 4, 4, 1) && mid < outlineRadius(Math.PI / 4, 5, 1));
  for (let t = 0; t < TAU; t += 0.05) assert.ok(outlineRadius(t, 48, 1) > 0.997);
});

test('a fractional count: the last shape fades in as the ring makes room; none past the most', () => {
  const k = { ...defaults(SHAPES), count: 2.5, opacity: 1, spread: 0.5, taper: 0 };
  const out = new Float32Array(MOST_SHAPES * SHAPE_FLOATS);
  assert.equal(shapeLayout(k, { orbit: 0, spin: 0 }, 1.6, 1, out), 3);
  assert.equal(out[2 * SHAPE_FLOATS + 7], 0.5);
  assert.equal(out[SHAPE_FLOATS + 7], 1);
  // The ring spaced by the count: the second shape a 2.5th of the way round.
  const a = Math.atan2(out[SHAPE_FLOATS + 1], out[SHAPE_FLOATS]);
  assert.ok(close(a, TAU / 2.5 - TAU, 1e-5) || close(a, TAU / 2.5, 1e-5), `${a}`);
  assert.equal(shapeLayout({ ...k, count: 99 }, { orbit: 0, spin: 0 }, 1.6, 1, out), MOST_SHAPES);
});

test('the bands’ wave: closes on itself, flat when there is nothing, within −1..1', () => {
  const v = new Float32Array(WAVE_POINTS);
  synthWave([0, 0, 0, 0, 0, 0], 1, v);
  assert.ok(v.every((x) => x === 0));
  synthWave([1, 1, 1, 1, 1, 1], 12.3, v);
  assert.ok(v.every((x) => x >= -1 && x <= 1));
  assert.ok(v.some((x) => Math.abs(x) > 0.1));
  // Whole cycles: the next sample after the last would be the first again, so the step across is a usual one.
  const step = Math.max(...v.map((x, i) => (i ? Math.abs(x - v[i - 1]) : 0)));
  assert.ok(Math.abs(v[0] - v[WAVE_POINTS - 1]) <= step + 1e-6);
  synthWave([NaN, 50, -1, 1, 1, 1], 0, v);
  assert.ok(v.every((x) => Number.isFinite(x) && x >= -1 && x <= 1));
});

test('resampling keeps both ends; the seam closes by the circle’s share', () => {
  const out = new Float32Array(8);
  resample([0, 1, 0.5], out);
  assert.equal(out[0], 0);
  assert.equal(out[7], 0.5);
  const v = new Float32Array(64).map((_, i) => i / 63);
  closeSeam(v, 1);
  assert.ok(close(v[63], v[0]));
  const w = new Float32Array(64).map((_, i) => i / 63);
  closeSeam(w, 0);
  assert.equal(w[63], 1);
});

test('the wave’s shapes: a line open at its ends, a circle closed, spokes out by the sample, mixed linearly', () => {
  const n = 32;
  const v = new Float32Array(n).map((_, i) => Math.sin((TAU * i) / n) * 0.5);
  const base = { ...defaults(WAVES), amplitude: 0.2, radius: 0.5, x: 0, y: 0, rotation: 0, length: 1 };
  const seg = (circle: number, radial: number) => {
    const out = new Float32Array(n * SEGMENT_FLOATS);
    waveSegments(v, { ...base, circle, radial }, 0, 1.5, 1, out);
    return out;
  };
  const line = seg(0, 0);
  assert.ok(close(line[0], -1.5) && close(line[(n - 2) * SEGMENT_FLOATS + 2], 1.5), 'across the width');
  assert.equal(line[(n - 1) * SEGMENT_FLOATS + 4], 0, 'the line’s ends unjoined');
  const ring = seg(1, 0);
  const last = (n - 1) * SEGMENT_FLOATS;
  assert.ok(close(ring[last + 2], ring[0], 1e-5) && close(ring[last + 3], ring[1], 1e-5), 'the circle closed');
  assert.equal(ring[last + 4], 1);
  const spokes = seg(0, 1);
  for (let i = 0; i < n; i++) {
    const o = i * SEGMENT_FLOATS;
    assert.ok(close(Math.hypot(spokes[o], spokes[o + 1]), 0.5, 1e-5));
    assert.ok(close(Math.hypot(spokes[o + 2], spokes[o + 3]), 0.5 + v[i] * 0.2, 1e-5));
  }
  const half = seg(0.5, 0);
  for (let j = 0; j < n * SEGMENT_FLOATS; j++) if (j % SEGMENT_FLOATS < 4) assert.ok(close(half[j], (line[j] + ring[j]) / 2, 1e-5));
});
