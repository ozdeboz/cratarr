// The infinite zooms' dive (zoom.ts): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diveCamera, diveFit, divePeriod, diveTarget } from '../src/lib/visuals/zoom.ts';
import { VARIATIONS, type Flame, type Xform } from '../src/lib/visuals/flame.ts';

const close = (a: number, b: number, eps = 1e-6) => Math.abs(a - b) < eps;
/** A part that is exactly a turn by `turn` and a shrink by `s` about the point `at`, as an affine map. */
const similar = (s: number, turn: number, at: [number, number], weight = 1): Xform => {
  const [c, d] = [s * Math.cos(turn), s * Math.sin(turn)];
  // x' = c·x − d·y + e, y' = d·x + c·y + f, with (e, f) so `at` stays put.
  const e = at[0] - (c * at[0] - d * at[1]);
  const f = at[1] - (d * at[0] + c * at[1]);
  return { affine: [c, -d, e, d, c, f], variations: new Map([[VARIATIONS.linear, 1]]), weight, color: 0, colorSpeed: 0, turns: false };
};
const flameOf = (xforms: Xform[]): Flame => ({ xforms, symmetry: { order: 1, mirror: false }, colours: { light: 0.7, chroma: 0.1 }, center: [0, 0], size: 1 });

test('a part that is exactly a turn and shrink is fitted exactly: its shrink, its turn, its fixed point', () => {
  const flame = flameOf([similar(0.5, 0.3, [0.4, -0.2]), similar(0.45, -1.1, [-0.5, 0.3]), similar(0.6, 2, [0.1, 0.6])]);
  const fit = diveFit(flame, 0)!;
  assert.ok(fit.error < 1e-9, `misfit ${fit.error}`);
  assert.ok(close(Math.hypot(...fit.a), 0.5) && close(Math.atan2(fit.a[1], fit.a[0]), 0.3));
  const cam = diveCamera(fit, 0.5, 0.2);
  assert.ok(close(cam.at[0], 0.4) && close(cam.at[1], -0.2), `fixed point ${cam.at}`);
});

test('the dive picks the part fitting best, within a fifth and nine tenths of a shrink', () => {
  // Part 0 too small (a tenth), part 2 nearly no shrink (0.95): neither; of 1 and 3, 3 is the exact one.
  const bent: Xform = { ...similar(0.5, 0, [0, 0]), variations: new Map([[VARIATIONS.linear, 0.6], [VARIATIONS.spherical, 0.4]]) };
  const flame = flameOf([similar(0.1, 0, [0.9, 0.9]), bent, similar(0.95, 0.5, [0, 0], 0.2), similar(0.55, 0.8, [-0.3, 0.2])]);
  const target = diveTarget(flame)!;
  assert.equal(target.j, 3);
});

test('the camera: none at the loop’s start, the part’s inverse at its end; a loop as long as the zoom takes', () => {
  const fit = diveFit(flameOf([similar(0.5, 0.3, [0.4, -0.2]), similar(0.4, 1, [-0.4, 0.3])]), 0)!;
  const start = diveCamera(fit, 0, 0.2);
  assert.ok(close(start.zoom[0], 1) && close(start.zoom[1], 0));
  const end = diveCamera(fit, 1, 0.2);
  // a^{-1}: 2 times as large, turned back by 0.3.
  assert.ok(close(Math.hypot(...end.zoom), 2, 1e-6) && close(Math.atan2(end.zoom[1], end.zoom[0]), -0.3, 1e-6));
  // At a rate of ln 2 a second, a loop that doubles takes a second.
  assert.ok(close(divePeriod(fit, Math.log(2)), 1, 1e-6));
  assert.ok(close(end.turnRate, -0.3 / divePeriod(fit, 0.2), 1e-9));
});

test('Presence: a part’s weight in the drawing scaled, the symmetry’s rotations as they were', async () => {
  const { evolved } = await import('../src/lib/visuals/flame.ts');
  const turning = { ...similar(0.5, 0.3, [0.4, -0.2]), turns: true };
  const flame = flameOf([turning, { ...turning }, similar(1, Math.PI / 2, [0, 0], 0.7)]);
  const parts = [
    { turn: 0, size: 1, morph: 0, hue: 0, weight: 2.5 },
    { turn: 0, size: 1, morph: 0, hue: 0 },
  ];
  const out = evolved(flame, parts);
  assert.equal(out.xforms[0].weight, 2.5, 'weighed by its presence');
  assert.equal(out.xforms[1].weight, 1, 'none given: as drawn');
  assert.equal(out.xforms[2].weight, 0.7, 'a rotation of the symmetry: as it was');
});

test('the dive’s turn foreseen: the part’s turn over the loop’s period, none without a dive', async () => {
  const { diveTurnOf } = await import('../src/lib/visuals/zoom.ts');
  const flame = flameOf([similar(0.5, 0.3, [0.4, -0.2]), similar(0.1, 0, [0.9, 0.9])]);
  assert.equal(diveTurnOf(flame, 0), 0);
  // A loop doubles the zoom: ln 2 / rate seconds, turning 0.3 in it.
  assert.ok(close(diveTurnOf(flame, 0.2), (0.3 * 0.2) / Math.log(2), 1e-6));
});
