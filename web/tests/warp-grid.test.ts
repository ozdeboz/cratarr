// The Warp's grid (loopmods.ts; the owner, 2026-10-29: "we're lacking a quality that MilkDrop has"): MilkDrop's
// per-vertex motion as continuous knobs; the old Warp unchanged at their defaults: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const loopmodsModule = await import('../src/lib/visuals/loopmods.ts');
const { WARP, WARP_GLSL, WarpClock, petalAt, warpFrame, warpFrom, warpUniforms } = loopmodsModule;

const defaults = (over: Record<string, number> = {}) => ({ ...Object.fromEntries(WARP.knobs.map((k) => [k.id, k.default])), ...over });
const still = { petal: 0, ring: 0 };
const close = (a: [number, number], b: [number, number], eps = 1e-12) => Math.abs(a[0] - b[0]) < eps && Math.abs(a[1] - b[1]) < eps;
const POINTS: [number, number][] = [[0, 0], [0.3, 0.1], [-0.6, 0.45], [0.8, -0.5], [0.01, -0.02], [-0.2, -0.33]];

test('at its defaults the grid moves nothing: every point is read from where it is', () => {
  const w = warpFrame([defaults()], 1 / 60, 16 / 9, 3.2, still);
  for (const p of POINTS) assert.ok(close(warpFrom(p, w), p), `${p}`);
  assert.deepEqual(w.drift, [0, 0]);
});

test('without the grid it moves as the Warp always did: zoom, turn, stretch and wobble about the middle', () => {
  // The Warp before the grid (compose.ts LOOP as it was).
  const old = (p: [number, number], k: Record<string, number>, dt: number, time: number): [number, number] => {
    let [x, y] = [p[0] / Math.exp(0.4 * k.zoom * dt), p[1] / Math.exp(0.4 * k.zoom * dt)];
    const t = Math.PI * k.turn * dt;
    [x, y] = [Math.cos(t) * x - Math.sin(t) * y, Math.sin(t) * x + Math.cos(t) * y];
    const s = Math.exp(0.2 * k.stretch * dt);
    [x, y] = [x / s, y * s];
    const wob = [0.02 * k.wobble * Math.min(1, dt * 30), 4 + 20 * k.wobbleSize, time * 1.7];
    return [x + wob[0] * Math.sin(y * wob[1] + wob[2]), y + wob[0] * Math.cos(x * wob[1] - wob[2])];
  };
  const k = defaults({ zoom: 0.7, turn: -0.4, stretch: 0.3, wobble: 0.5, wobbleSize: 0.2, driftX: 0.25, driftY: -0.5 });
  const dt = 1 / 50;
  const w = warpFrame([k], dt, 4 / 3, 7.5, still);
  for (const p of POINTS) assert.ok(close(warpFrom(p, w), old(p, k, dt, 7.5), 1e-12), `${p}`);
  assert.deepEqual(w.drift, [0.2 * 0.25 * dt, 0.2 * -0.5 * dt]);
});

test('zoom at the edge: the centre stays, and the farther a point the more it is zoomed', () => {
  const w = warpFrame([defaults({ zoomEdge: 1 })], 1 / 30, 1, 0, still);
  assert.ok(close(warpFrom([0, 0], w), [0, 0]));
  const moved = (p: [number, number]) => Math.hypot(...p) - Math.hypot(...warpFrom(p, w));
  assert.ok(moved([0.05, 0]) > 0 && moved([0.05, 0]) < moved([0.25, 0]) && moved([0.25, 0]) < moved([0.5, 0]), 'pulled in from farther the farther out');
});

test('turn at the edge: a swirl, the centre still and the rim turned most', () => {
  const w = warpFrame([defaults({ turnEdge: 1 })], 1 / 30, 1, 0, still);
  const angle = (p: [number, number]) => {
    const q = warpFrom(p, w);
    return Math.atan2(q[1], q[0]) - Math.atan2(p[1], p[0]);
  };
  assert.ok(Math.abs(angle([0.02, 0])) < Math.abs(angle([0.4, 0])), 'turned more out at the rim');
  const q = warpFrom([0.4, 0], w);
  assert.ok(Math.abs(Math.hypot(...q) - 0.4) < 1e-12, 'a turn only, no zoom');
});

test('the centre moves where it zooms and turns about: it stays put itself', () => {
  const aspect = 16 / 9;
  const w = warpFrame([defaults({ zoom: 1, turn: 1, zoomEdge: 0.5, centreX: 0.5, centreY: -0.4 })], 1 / 20, aspect, 0, still);
  const c: [number, number] = [0.5 * aspect * 0.5, 0.5 * -0.4];
  assert.ok(close(warpFrom(c, w), c, 1e-12));
  assert.ok(!close(warpFrom([0, 0], w), [0, 0], 1e-6), 'the middle moves now');
});

test('petals: a count between two whole ones fades the next petal in, with no step and no seam', () => {
  for (const th of [0, 0.4, 1.3, 2.9, -2.2]) {
    assert.ok(Math.abs(petalAt(th, 3.9999, 0.3) - petalAt(th, 4, 0.3)) < 1e-3, 'continuous in the count');
    assert.ok(Math.abs(petalAt(th, 4, 0.3) - Math.cos(4 * th + 0.3)) < 1e-12, 'a whole count its own petals');
  }
  for (const count of [2.5, 4.3, 7.75]) {
    assert.ok(Math.abs(petalAt(Math.PI - 1e-9, count, 0.2) - petalAt(-Math.PI + 1e-9, count, 0.2)) < 1e-6, `no seam at ${count}`);
  }
  const w = warpFrame([defaults({ petals: 1, petalCount: 4 })], 1 / 30, 1, 0, still);
  const r = (p: [number, number]) => Math.hypot(...warpFrom(p, w));
  // Four petals: zoomed in along the axes (read from nearer), out between them.
  assert.ok(r([0.3, 0]) < 0.3 && r([0, 0.3]) < 0.3 && r([0.3 * Math.SQRT1_2, 0.3 * Math.SQRT1_2]) > 0.3);
});

test('rings run out from the centre: the same place zooms in and out as the rings pass', () => {
  const clock = new WarpClock();
  const k = defaults({ rings: 1, ringCount: 3, ringSpeed: 0.5 });
  const radii: number[] = [];
  for (let i = 0; i < 30; i++) {
    clock.run(k, 1 / 30);
    radii.push(Math.hypot(...warpFrom([0.2, 0], warpFrame([k], 1 / 30, 1, 0, clock))));
  }
  assert.ok(radii.some((r) => r < 0.2) && radii.some((r) => r > 0.2), 'in, then out, as they pass');
});

test("a change of speed goes on from where the clocks are: no jump", () => {
  const c = new WarpClock();
  c.run(defaults({ petalTurn: 1, ringSpeed: 1 }), 0.5);
  const at = { petal: c.petal, ring: c.ring };
  c.run(defaults({ petalTurn: -1, ringSpeed: -1 }), 1 / 1000);
  assert.ok(Math.abs(c.petal - at.petal) < 0.01 && Math.abs(c.ring - at.ring) < 0.02);
});

test("the shader's uniforms are the frame's, each declared in its GLSL, and there are no arrays", () => {
  const u = warpUniforms(warpFrame([defaults()], 1 / 60, 1, 0, still));
  for (const n of Object.keys(u)) assert.match(WARP_GLSL, new RegExp(`uniform \\w+ ${n};`), n);
  assert.doesNotMatch(WARP_GLSL, /\[/, 'no arrays: Mali miscompiles them');
  assert.match(WARP_GLSL, /vec2 warpFrom\(vec2 p\)/);
});

test('every new knob is continuous, and each does nothing at its default but its counts and speeds', () => {
  const grid = ['centreX', 'centreY', 'zoomEdge', 'turnEdge', 'petals', 'petalCount', 'petalTurn', 'rings', 'ringCount', 'ringSpeed', 'turbulence', 'turbulenceSpeed', 'turbulenceSize'];
  for (const id of grid) {
    const k = WARP.knobs.find((x) => x.id === id);
    assert.ok(k, id);
    assert.notEqual(k.kind, 'step', `${id} glides`);
  }
});

test("MilkDrop's turbulence: none at 0, a point moved at most twice its amount, and its time run on without a jump", () => {
  const { turbulenceOf } = loopmodsModule;
  const still2 = { petal: 0, ring: 0, turbulence: 12.5 };
  const off = warpFrame([defaults()], 1 / 60, 1, 0, still2);
  for (const p of POINTS) assert.ok(close(warpFrom(p, off), p), 'none at 0');
  const on = warpFrame([defaults({ turbulence: 1 })], 1 / 60, 1, 0, still2);
  const most = 2 * on.turbulence.amount;
  let moved = 0;
  for (const p of POINTS) {
    const q = warpFrom(p, on);
    const d = Math.hypot(q[0] - p[0], q[1] - p[1]);
    assert.ok(d <= most * Math.SQRT2 + 1e-12, 'at most its amount on each axis, twice');
    moved = Math.max(moved, d);
  }
  assert.ok(moved > 0, 'it moves them');
  // MilkDrop's own numbers at 1 (0.0035 of the picture a frame at 60 frames a second).
  assert.ok(Math.abs(turbulenceOf(1, 1, 0, 1 / 60).amount - 0.0035) < 1e-12);
  const c = new WarpClock();
  c.run(defaults({ turbulenceSpeed: 2 }), 3);
  const t = c.turbulence;
  c.run(defaults({ turbulenceSpeed: 0.1 }), 1 / 1000);
  assert.ok(Math.abs(c.turbulence - t) < 0.001, 'a change of speed goes on from where its time is');
});
