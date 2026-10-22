// effect-modules.ts placeEffect (decision 0057 §3): an effect set on a slot of a converted patch, as the dice and the
// moments set one: the patch wired as toV2 would wire those settings (picture-graph.ts compile the judge): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
await import('../src/lib/visuals/picture-wiring.ts');
const { compile, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EFFECTS, engineEffect, loopEffect } = await import('../src/lib/visuals/effects.ts');
const { effectKnobIds, effectType, placeEffect, slotModule } = await import('../src/lib/visuals/effect-modules.ts');
type VisualSettings = typeof DEFAULTS;
type PatchV2 = ReturnType<typeof toV2>;

const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
const v2 = (s: Partial<VisualSettings> = {}) => toV2({ ...JSON.parse(JSON.stringify(DEFAULTS)), ...s });
// What is drawn: each module with what feeds it, and the effects' knobs (by id: the order follows from the feeds).
const drawn = (p: PatchV2) => {
  const c = compile(p, types);
  assert.deepEqual(c.errors, []);
  const knobs = (id: string) => p.modules.find((m) => m.id === id)?.knobs;
  return c.order.map((n) => [n.id, n.type, n.inputs.map((f) => `${f.port}<${f.from.module}.${f.from.port}`).join(' '), n.type.startsWith('fx-') ? knobs(n.id) : null]).sort((a, b) => String(a[0]).localeCompare(String(b[0])));
};
const ROTATE = engineEffect('kaleidoscope', 3);
const PULSE = engineEffect('eyedo', 1);
const ECHO = engineEffect('hydra', 1);

test('added, one by one, the effects are wired as toV2 wires them: in their order, Echo in its visible loop', () => {
  let p = v2();
  p = placeEffect(p, 'fx3', ECHO, { amount: 0.8 });
  assert.deepEqual(drawn(p), drawn(v2({ fx3: 1, fx3Amount: 0.8 })), 'an Echo alone: in its loop too');
  assert.ok(p.modules.some((m) => m.id === `${effectType(EFFECTS.find((e) => e.id === ECHO)!)}-feedback`));
  p = placeEffect(p, 'fx1', ROTATE, { amount: 1 });
  assert.deepEqual(drawn(p), drawn(v2({ fx1: 3, fx1Amount: 1, fx3: 1, fx3Amount: 0.8 })), 'a shape after it: the loop round it');
  p = placeEffect(p, 'fx2', PULSE, { amount: 0.8 });
  assert.deepEqual(drawn(p), drawn(v2({ fx1: 3, fx1Amount: 1, fx2: 1, fx2Amount: 0.8, fx3: 1, fx3Amount: 0.8 })));
});

test('taken out (none, or at nothing), the line closes; an Echo keeps its loop, which goes with it', () => {
  const all = v2({ fx1: 3, fx1Amount: 1, fx2: 1, fx2Amount: 0.8, fx3: 1, fx3Amount: 0.8 });
  const noPulse = placeEffect(all, 'fx2', 0, { amount: 0 });
  assert.deepEqual(drawn(noPulse), drawn(v2({ fx1: 3, fx1Amount: 1, fx3: 1, fx3Amount: 0.8 })));
  const echoOnly = placeEffect(noPulse, 'fx1', ROTATE, { amount: 0 });
  assert.deepEqual(drawn(echoOnly), drawn(v2({ fx3: 1, fx3Amount: 0.8 })));
  assert.ok(echoOnly.modules.some((m) => /-feedback$/.test(m.id) && m.id.startsWith('fx-')), 'the loop kept');
  const none = placeEffect(echoOnly, 'fx3', 0, { amount: 1 });
  assert.deepEqual(drawn(none), drawn(v2()));
  assert.ok(!none.modules.some((m) => /-(mix|feedback)$/.test(m.id) && m.id.startsWith('fx-')), 'the loop gone with it');
  assert.deepEqual(drawn(placeEffect(v2(), 'fx1', 0, { amount: 0 })), drawn(v2()), 'nothing to take out');
});

test('swapped: the new effect in its place, the cables into its knobs and its pins carried by their place', () => {
  const kaleido = EFFECTS.filter((e) => e.engine === 'kaleidoscope' && e.slots.includes('picture'));
  const two = kaleido.filter((e) => e.knobs.length === 2 && e.id !== ROTATE);
  const fewer = kaleido.find((e) => e.knobs.length < 2);
  const rotate = EFFECTS.find((e) => e.id === ROTATE)!;
  const p = v2({ fx1: 3, fx1Amount: 1, fx1K1: 0.2, fx1K2: 0.7, fx3: 1, fx3Amount: 0.8 });
  const id = effectType(rotate);
  const [k1, k2] = effectKnobIds(rotate);
  const lfo = (port: string) => ({ id: `lfo>${port}`, from: { module: 'music', port: 'level' }, to: { module: id, port }, value: 0.5 });
  const wired = { ...p, cables: [...p.cables, lfo('amount'), lfo(k1), ...(k2 ? [lfo(k2)] : [])], pins: [...p.pins, `${id}.${k1}`, `${id}.amount`] };
  const into = two[0];
  const q = placeEffect(wired, 'fx1', into.id, { amount: 0.6 });
  const m = slotModule(q, 'fx1')!;
  assert.equal(m.id, effectType(into));
  assert.deepEqual(m.knobs, { amount: 0.6, ...Object.fromEntries(effectKnobIds(into).map((n, i) => [n, [0.2, 0.7][i]])) }, 'its knobs the old ones by place');
  assert.ok(!q.modules.some((x) => x.id === id) && !q.cables.some((c) => c.to.module === id || c.from.module === id));
  const [n1, n2] = effectKnobIds(into);
  assert.deepEqual(q.cables.filter((c) => c.id.startsWith('lfo>')).map((c) => c.to.port), ['amount', n1, ...(k2 ? [n2] : [])]);
  assert.ok(q.pins.includes(`${m.id}.${n1}`) && q.pins.includes(`${m.id}.amount`));
  assert.deepEqual(drawn(q).map((x) => x.slice(0, 3)), drawn(v2({ fx1: kaleido.indexOf(into) + 1, fx1Amount: 0.6, fx3: 1, fx3Amount: 0.8 })).map((x) => x.slice(0, 3)));
  if (fewer) {
    const r = placeEffect(wired, 'fx1', fewer.id, { amount: 1 });
    const kept = effectKnobIds(fewer);
    for (const c of r.cables.filter((x) => x.id.startsWith('lfo>'))) assert.ok(c.to.port === 'amount' || kept.includes(c.to.port), 'a knob it has not: dropped');
  }
  // An effect of another engine is not this slot's.
  assert.deepEqual(placeEffect(p, 'fx1', PULSE, { amount: 1 }), p);
});

test("the loop's effect: on the flame's light between Tunnel and Dive, swapped and taken out", () => {
  const loops = EFFECTS.filter((e) => e.slots.includes('loop'));
  const first = loopEffect(1);
  let p = placeEffect(v2(), 'loop-effect', first, { amount: 0.8 });
  assert.deepEqual(drawn(p), drawn(v2({ fxLoop: 1, fxLoopAmount: 0.8 })));
  if (loops.length > 1) {
    p = placeEffect(p, 'loop-effect', loopEffect(2), { amount: 0.8 });
    assert.deepEqual(drawn(p), drawn(v2({ fxLoop: 2, fxLoopAmount: 0.8 })));
    assert.ok(slotModule(p, 'loop-effect')!.id.startsWith('loop-'));
  }
  assert.deepEqual(drawn(placeEffect(p, 'loop-effect', 0, { amount: 0 })), drawn(v2()));
});

test('pure: the patch given is left as it was', () => {
  const p = v2({ fx1: 3, fx1Amount: 1, fx3: 1, fx3Amount: 0.8 });
  const before = structuredClone(p);
  placeEffect(p, 'fx2', PULSE, { amount: 1 });
  placeEffect(p, 'fx1', 0, { amount: 0 });
  placeEffect(p, 'loop-effect', loopEffect(1), { amount: 1 });
  assert.deepEqual(p, before);
});
