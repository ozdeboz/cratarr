// The Fluid's Fire (fluid.ts fireOf; 0.9.18, the owner via fire-e, 2026-10-29): Lift, Open walls and
// Cooling, each in its range; at none of them the fluid steps as it always did (six iterations, 0.8 of the pressure
// kept), so a rack without them looks as before: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { COOLING_MOST, LIFT_MOST, fireOf } = await import('../src/lib/visuals/fluid.ts');
const heat = { light: {} as WebGLTexture, k: 1 };

test('none of it: the fluid as it was', () => {
  assert.deepEqual(fireOf({}, heat), { lift: 0, open: 0, cooling: 0, iterations: 6, warm: 0.8 });
  assert.deepEqual(fireOf({ lift: 0, open: 0, cooling: 0 }), { lift: 0, open: 0, cooling: 0, iterations: 6, warm: 0.8 });
});

test('open: the pressure solved as an open box needs, by as much as it is open', () => {
  assert.deepEqual(fireOf({ open: 1 }), { lift: 0, open: 1, cooling: 0, iterations: 20, warm: 1 });
  const half = fireOf({ open: 0.5 });
  assert.equal(half.iterations, 13);
  assert.ok(Math.abs(half.warm - 0.9) < 1e-12);
});

test('each within its range; Lift only with heat to read', () => {
  assert.equal(fireOf({ lift: 2 }, heat).lift, 2);
  assert.equal(fireOf({ lift: 2 }).lift, 0, 'no light to read: no lift');
  assert.equal(fireOf({ lift: 99 }, heat).lift, LIFT_MOST);
  assert.equal(fireOf({ cooling: -1 }).cooling, 0);
  assert.equal(fireOf({ cooling: 99 }).cooling, COOLING_MOST);
  assert.equal(fireOf({ open: Number.NaN }).open, 0);
});
