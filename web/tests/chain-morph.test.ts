// The extras through the morph (chain-morph.ts, decision 0056 step 6): what fades out is drawn where it stood until it
// is gone; the evaluator reads the knobs as the morph shows them: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { morphedPatch, morphModules, rolledExtra, sameModules, withLeaving } from '../src/lib/visuals/chain-morph.ts';
import { morphTo, newMorph, stepMorph } from '../src/lib/visuals/morph.ts';
import { emptyExtra, type Knob, type Layer, type Module, type ModuleType, type Patch, type PatchExtra } from '../src/lib/visuals/patch.ts';

const mod = (id: string, type: string, knobs: Record<string, number> = { amount: 1 }): Module => ({ id, type, knobs });
const layer = (id: string, modules: Module[] = [], loop: Module[] = []): Layer => ({ id, source: mod(`${id}-src`, 'shapes'), modules, loop, blend: { mode: 'add', amount: 1 } });
const extra = (p: Partial<PatchExtra>): PatchExtra => ({ ...emptyExtra(), ...p });
const ids = (ms: { id: string }[]) => ms.map((m) => m.id);

test('nothing leaving: the extras as they are', () => {
  const now = extra({ layers: [layer('a')] });
  assert.equal(withLeaving(now, extra({}), new Set()), now);
});

test('a module gone from a layer is drawn where it stood, after the one before it', () => {
  const was = extra({ layers: [layer('a', [mod('k', 'kaleidoscope-engine'), mod('e', 'eyedo-engine'), mod('h', 'hydra-engine')])] });
  const now = extra({ layers: [layer('a', [mod('k', 'kaleidoscope-engine'), mod('h', 'hydra-engine')])] });
  const shown = withLeaving(now, was, new Set(['e']));
  assert.deepEqual(ids(shown.layers[0].modules), ['k', 'e', 'h']);
  // Once let go, not.
  assert.deepEqual(ids(withLeaving(now, was, new Set(['x'])).layers[0].modules), ['k', 'h']);
});

test('a layer gone whole is drawn while its source fades; the picture loop keeps its leaving modules in order', () => {
  const was = extra({ layers: [layer('a'), layer('b', [mod('k', 'kaleidoscope-engine')])], loop: [mod('w', 'warp'), mod('f', 'fluid-loop'), mod('kp', 'keep')] });
  const now = extra({ layers: [layer('a')], loop: [mod('w', 'warp'), mod('kp', 'keep')] });
  const shown = withLeaving(now, was, new Set(['b-src', 'k', 'f']));
  assert.deepEqual(ids(shown.layers), ['a', 'b']);
  assert.deepEqual(ids(shown.layers[1].modules), ['k']);
  assert.deepEqual(ids(shown.loop), ['w', 'f', 'kp']);
});

test('a module whose id the new chain gave another type leaves at once (one id, one running module)', () => {
  const was = extra({ loop: [mod('m', 'fluid-loop')] });
  const now = extra({ loop: [mod('m', 'warp')] });
  assert.deepEqual(ids(withLeaving(now, was, new Set(['m'])).loop), ['m']);
  assert.equal(withLeaving(now, was, new Set(['m'])).loop[0].type, 'warp');
});

test('the evaluator reads the knobs as the morph shows them; the flame’s modules untouched', () => {
  const flame = mod('flame', 'flame', { hue: 0.3 });
  const p: Patch = { v: 1, frame: mod('frame', 'frame'), layers: [{ ...layer('flame'), source: flame }, layer('a', [mod('k', 'kaleidoscope-engine')])], loop: [mod('w', 'warp')], finish: [], modulators: [mod('lfo', 'lfo')], cables: [], macros: [], pins: [] } as unknown as Patch;
  const q = morphedPatch(p, { k: { amount: 0.25 }, w: { amount: 0.5 }, lfo: { rate: 2 } });
  assert.equal(q.layers[0].source, flame);
  assert.equal(q.layers[1].modules[0].knobs.amount, 0.25);
  assert.equal(q.loop[0].knobs.amount, 0.5);
  assert.equal(q.modulators[0].knobs.rate, 2);
  assert.equal(p.loop[0].knobs.amount, 1, 'the patch itself as it was');
});

test('a chain change is one of modules or layers, not of knobs', () => {
  const a = extra({ layers: [layer('a', [mod('k', 'kaleidoscope-engine', { amount: 0.2 })])] });
  const b = extra({ layers: [layer('a', [mod('k', 'kaleidoscope-engine', { amount: 0.9 })])] });
  assert.ok(sameModules(a, b));
  assert.ok(!sameModules(a, extra({ layers: [layer('a')] })));
  assert.ok(!sameModules(a, extra({ layers: [layer('a', [mod('k', 'eyedo-engine')])] })));
});

test('through the morph: a removed module fades out drawn, then is let go', () => {
  const knob = (id: string): Knob => ({ id, label: id, min: 0, max: 1, neutral: 0, default: 1, kind: 'level' });
  const TYPES: Record<string, ModuleType> = Object.fromEntries(
    ['shapes', 'kaleidoscope-engine'].map((t) => [t, { type: t, label: t, place: 'module', knobs: [knob('amount')] } as ModuleType]),
  );
  const was = extra({ layers: [layer('a', [mod('k', 'kaleidoscope-engine')])] });
  const now = extra({ layers: [layer('a')] });
  const m = newMorph(morphModules(was), TYPES);
  morphTo(m, morphModules(now), 1, TYPES);
  let f = stepMorph(m, 0.5, null, TYPES);
  const going = new Set(f.leaving.map((l) => l.id));
  assert.deepEqual([...going], ['k']);
  assert.deepEqual(ids(withLeaving(now, was, going).layers[0].modules), ['k']);
  assert.ok(f.leaving[0].knobs.amount > 0 && f.leaving[0].knobs.amount < 1);
  f = stepMorph(m, 0.6, null, TYPES);
  assert.equal(f.leaving[0]?.knobs.amount ?? 0, 0);
  f = stepMorph(m, 0.1, null, TYPES);
  assert.equal(f.leaving.length, 0);
});

test('the dice within a chain: every look knob anew in its range, the chain and the modulators as they are', () => {
  const knob = (id: string, kind: Knob['kind'], min: number, max: number, neutral: number, device = false): Knob => ({ id, label: id, min, max, neutral, default: neutral, kind, ...(device ? { device } : {}) });
  const TYPES = {
    shapes: { knobs: [knob('amount', 'level', 0, 1, 0), knob('spin', 'swing', -1, 1, 0), knob('count', 'step', 1, 6, 1), knob('quality', 'level', 0, 2, 1, true)] },
    lfo: { knobs: [knob('rate', 'swing', 0, 4, 1)] },
  };
  const e = extra({ layers: [{ ...layer('a'), source: mod('s', 'shapes', { amount: 0.1, spin: 0, count: 2, quality: 1 }) }], modulators: [mod('l', 'lfo', { rate: 1 })] });
  for (const r of [0, 0.5, 0.999]) {
    const out = rolledExtra(e, TYPES, () => r);
    const k = out.layers[0].source.knobs;
    assert.ok(k.amount >= 0 && k.amount <= 0.75 && k.spin >= -1 && k.spin <= 1 && Number.isInteger(k.count) && k.count >= 1 && k.count <= 6, JSON.stringify(k));
    assert.equal(k.quality, 1);
    assert.equal(out.modulators[0], e.modulators[0]);
    assert.ok(sameModules(out, e));
  }
  assert.equal(e.layers[0].source.knobs.amount, 0.1, 'the extras themselves as they were');
});
