// The one macro rule (lib/visuals/macro-map.ts; the architecture audit, 2026-10-29: one resolver, six before): a target
// along its taper, the last macro on a knob its own; the patch as set reads a device's macro under the rack's the same.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { macroAt, mapped, throughMacros } = await import('../src/lib/visuals/macro-map.ts');
const { asSet, macroAsSet } = await import('../src/lib/visuals/v2.ts');
import type { PatchV2 } from '../src/lib/visuals/ports.ts';

test('a target between its from and its to, an exponential one geometrically', () => {
  assert.equal(mapped({ from: 2, to: 6 }, 0.5), 4);
  assert.equal(mapped({ from: 0.01, to: 1 }, 0.5, true), 0.1);
  // Geometric only where both ends are above 0.
  assert.equal(mapped({ from: 0, to: 1 }, 0.25, true), 0.25);
  assert.equal(macroAt(1.4), 1);
  assert.equal(macroAt(-0.2), 0);
});

test('several on one knob: the last one sets it', () => {
  const targets = [
    { from: 0, to: 1, v: 0.2 },
    { from: 10, to: 20, v: 0.5 },
  ];
  assert.equal(throughMacros(7, targets, (t) => t.v), 15);
  assert.equal(throughMacros(7, [], () => 1), 7, 'none: its own');
});

test("as set: a device's macro under the rack's, through the same rule", () => {
  const p = {
    v: 2,
    modules: [{ id: 'd/glow', type: 'glow', knobs: { amount: 0.3 } }],
    cables: [],
    macros: [
      { id: 'd/macro:inner', on: 'd', label: 'Inner', value: 0.9, targets: [{ module: 'd/glow', knob: 'amount', from: 0, to: 2 }] },
      { id: 'outer', on: 'rack', label: 'Outer', value: 0.5, targets: [{ module: 'd/macro:inner', knob: 'value', from: 0.2, to: 0.6 }] },
    ],
    pins: [],
  } as unknown as PatchV2;
  const inner = p.macros[0];
  assert.ok(Math.abs(macroAsSet(p, inner) - 0.4) < 1e-12, "the outer one's sets the inner one's value");
  assert.ok(Math.abs(asSet(p, p.modules[0], 'amount')! - 0.8) < 1e-12, 'and the knob from it');
});
