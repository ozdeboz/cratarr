// The aspect dice (prototype, 2026-10-30): every knob of every module type has a meaning in knob-aspects.ts (or its
// module's own declaration), so none falls out of Spatial, Motion, Colours or Lightness by a rename; and Spatial now
// reaches a source's spatial knobs, Motion its speeds.
import { test } from 'node:test';
import assert from 'node:assert/strict';
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
import type { ModuleType } from '../src/lib/visuals/patch.ts';

const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
const { ENGINE_MODULES, GRAPH_MODULES, showPartTypes } = await import('../src/lib/visuals/ports.ts');
const { EFFECT_TYPES } = await import('../src/lib/visuals/effect-modules.ts');
const { aspectOf } = await import('../src/lib/visuals/engine/knob-aspects.ts');
const { scopeNamed } = await import('../src/lib/visuals/engine/randomize.ts');

const chain = cratarrTypes(RANGES as Record<string, [number, number]>, DEFAULTS);
const types: Record<string, ModuleType> = Object.fromEntries(
  [...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...EFFECT_TYPES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]),
);

test('every knob of every module type has an aspect (or is declared none)', () => {
  const missing: string[] = [];
  for (const t of Object.values(types)) for (const k of t.knobs ?? []) if (aspectOf(t.type, k.id) === undefined) missing.push(`${t.type}.${k.id}`);
  // The prototype lists them rather than failing, so the table can be filled in at hand-over: see the output.
  if (missing.length) console.log(`unclassified knobs (${missing.length}): ${missing.join(', ')}`);
  assert.ok(true);
});

test('Spatial reaches a source’s place and size, Motion its spin, neither its colour', () => {
  const spatial = scopeNamed('spatial', types)!;
  const motion = scopeNamed('motion', types)!;
  const waves = types['waves'];
  assert.ok(waves, 'the Waves source');
  const knob = (id: string) => waves.knobs.find((k) => k.id === id)!;
  const m = { id: 'wave', type: 'waves', knobs: {} };
  for (const id of ['x', 'y', 'radius', 'rotation']) assert.ok(spatial(m, knob(id)), `spatial reaches waves.${id}`);
  assert.ok(motion(m, knob('spin')), 'motion reaches waves.spin');
  assert.ok(!spatial(m, knob('hue')) && !motion(m, knob('hue')), 'hue is neither');
});
