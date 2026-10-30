// The Gradient source (sources/gradient.ts; the owner, 2026-10-29: continuous controls, any grid we want): its fields
// by place, blended and not switched, petals with no seam, bands that fade in: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { GRADIENT, banded, centreOf, gradientAt } = await import('../src/lib/visuals/sources/gradient.ts');
const { SOURCE_TYPES } = await import('../src/lib/visuals/sources/index.ts');

const k = (over: Record<string, number> = {}) => ({ ...Object.fromEntries(GRADIENT.knobs.map((x) => [x.id, x.default])), ...over });

test('a source like the others, every knob continuous', () => {
  assert.ok(SOURCE_TYPES.some((t) => t.type === 'gradient'));
  for (const x of GRADIENT.knobs) assert.notEqual(x.kind, 'step', `${x.id} glides`);
});

test('by default the distance from the middle: dark there, light at the short side’s edge; none at no amount', () => {
  assert.equal(gradientAt([0, 0], k(), 16 / 9, 0), 0);
  assert.ok(Math.abs(gradientAt([0, 1], k(), 16 / 9, 0) - 1) < 1e-12);
  assert.ok(Math.abs(gradientAt([0.5, 0], k(), 16 / 9, 0) - 0.5) < 1e-12);
  assert.equal(gradientAt([0.3, 0.2], k({ amount: 0 }), 1, 0), 0);
});

test('the fields blend: a share of each, summed, not one of them chosen', () => {
  const at: [number, number] = [0.4, -0.2];
  const d = gradientAt(at, k({ distance: 1, across: 0 }), 1, 0);
  const a = gradientAt(at, k({ distance: 0, across: 1 }), 1, 0);
  const both = gradientAt(at, k({ distance: 0.5, across: 0.5 }), 1, 0);
  assert.ok(Math.abs(both - (0.5 * d + 0.5 * a)) < 1e-12);
});

test('round the centre: a count between two whole ones fades the next petal in, with no seam', () => {
  const round = (p: [number, number], count: number) => gradientAt(p, k({ distance: 0, round: 1, roundCount: count }), 1, 0);
  for (const p of [[0.3, 0.1], [-0.2, 0.4], [-0.5, -0.05]] as [number, number][]) {
    assert.ok(Math.abs(round(p, 2.9999) - round(p, 3)) < 1e-3, 'continuous in the count');
  }
  for (const count of [1.5, 3.25, 6.8]) {
    assert.ok(Math.abs(round([-0.4, 1e-9], count) - round([-0.4, -1e-9], count)) < 1e-6, `no seam at ${count}`);
  }
});

test('bands fade in from the plain field and move with their place, with no step', () => {
  assert.equal(banded(0.3, 0, 0), 0.3);
  assert.ok(Math.abs(banded(0.3, 1e-6, 0) - 0.3) < 1e-5, 'none to speak of just above 0');
  assert.ok(Math.abs(banded(0.3, 3, 0.25) - banded(0.3, 3, 0.2500001)) < 1e-5);
  assert.ok(Math.abs(banded(0.3, 3, 0) - banded(0.3 + 1 / 3, 3, 0)) < 1e-12, 'a band every third of the field');
});

test('the centre reaches each edge of its own side, wide or tall', () => {
  assert.deepEqual(centreOf(k({ centreX: 1, centreY: 1 }), 2), [2, 1]);
  assert.deepEqual(centreOf(k({ centreX: 1, centreY: 1 }), 0.5), [1, 2]);
  assert.equal(gradientAt([1.6, 0], k({ centreX: 0.8 }), 2, 0), 0, 'dark at its centre, wherever it is');
});
