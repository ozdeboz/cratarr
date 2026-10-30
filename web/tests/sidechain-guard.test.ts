// The Screen's flash guard (0057 §7e; WCAG 2.3.1): a strobe held to a tenth at once and for 8 s after it stops, no
// knob in it. (Kept from the sidechain proofs, which held the sidechains to the governor over the old engine: retired
// with it in 0.10, the golden frames' closed loop guarding the sidechains since.)
import { test } from 'node:test';
import assert from 'node:assert/strict';
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { newSidechain, stepSidechain, FLASH_GUARD, STANDING } = await import('../src/lib/visuals/engine/sidechain.ts');

test('the flash guard: a strobe held to a tenth at once, held 8 s after it stops, a trial included, no knob in it', () => {
  const s = newSidechain();
  const quiet = { flash: 0, redFlash: 0, motion: 0, shapeChurn: 0, colourChurn: 0, zoomPumping: 0, looksPerMinute: 0 } as unknown as import('../src/lib/visuals/meters.ts').MeterValues;
  const strobe = { ...quiet, flash: 6 };
  const opts = { dt: 1 / 30, limits: STANDING, wild: 1, activity: 1, budgets: false };
  let l = stepSidechain(s, strobe, opts);
  assert.ok(Math.abs(l.swing - 0.1) < 1e-9, `at once: ${l.swing}`);
  for (let k = 0; k < 30 * 3; k++) l = stepSidechain(s, strobe, opts);
  assert.ok(Math.abs(l.swing - 0.1) < 1e-9);
  for (let k = 0; k < 30 * 7.9; k++) l = stepSidechain(s, quiet, opts);
  assert.ok(Math.abs(l.swing - 0.1) < 1e-9, `held 8 s: ${l.swing}`);
  for (let k = 0; k < 30 * 15; k++) l = stepSidechain(s, quiet, opts);
  assert.ok(l.swing > 0.95, `let back: ${l.swing}`);
  assert.ok(Object.isFrozen(FLASH_GUARD) || FLASH_GUARD.threshold === 2.5);
});
