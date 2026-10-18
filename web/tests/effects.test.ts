// The effects stage's chains (effects.ts): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chainOf, EFFECTS, loopFunction, pictureShader, reportOf } from '../src/lib/visuals/effects.ts';

test('every effect is one function of its kind, named after it, and its id is its own', () => {
  const ids = new Set<number>();
  for (const e of EFFECTS) {
    assert.ok(!ids.has(e.id), `id ${e.id} once`);
    ids.add(e.id);
    const head = { shape: `vec2 fx_${e.name}(vec2 p, float a, float t, vec2 k)`, colour: `vec3 fx_${e.name}(vec3 c, vec2 p, float a, float t, vec2 k)`, echo: `vec3 fx_${e.name}(vec3 c, vec2 p, float a, float t, vec2 k, sampler2D prev)` }[e.kind];
    assert.ok(e.glsl.includes(head), `${e.name}: ${head}`);
    assert.ok(!/\bdiscard\b/.test(e.glsl) && !/\btexture\(/.test(e.glsl), `${e.name}: no discard, no implicit-LOD reads`);
    if (e.slots.includes('loop')) assert.equal(e.kind, 'shape', `${e.name}: the loop takes shapes only`);
  }
});

test('a chain: shapes before the read, colours after, each by its place; nothing in it, no shader', () => {
  assert.equal(pictureShader([0, 0, 0]), null);
  assert.equal(pictureShader([999]), null, 'an effect not known shows nothing');
  const s = pictureShader([2, 1, 0])!;
  const read = s.indexOf('vec3 c = textureLod(uPicture');
  assert.ok(s.indexOf('p = fx_kaleid(p, uFxA[1], uFxT, uFxK[1]);') < read, 'the shape first, by its place');
  assert.ok(s.indexOf('c = fx_posterize(c, p, uFxA[0], uFxT, uFxK[0]);') > read, 'the colour after');
  assert.equal(s.split('vec2 fx_kaleid(').length, 2, 'each function once');
  assert.deepEqual(chainOf([1, 2, 999], 'loop').map((e) => e.name), ['kaleid'], 'a colour and one not known: not for the loop');
});

test('the loop: a shape on the buffer’s coordinates, else the identity; reports add up by amount', () => {
  assert.ok(loopFunction(1).includes('p = fx_kaleid(p, a, t, k);'));
  assert.ok(loopFunction(2).includes('return uv;'), 'a colour is not for the loop');
  // Kaleid at its knobs' middles: no spin, so no motion; its spin knob turned up, as fast as it turns.
  const r = reportOf([1, 1], [0.5, 1], 'picture');
  assert.ok(r.motion === 0 && Math.abs(r.shapeChurn! - 0.15) < 1e-12, JSON.stringify(r));
  const spun = reportOf([1], [1], 'picture', [[0.5, 1]]);
  assert.ok(Math.abs(spun.motion! - 0.2) < 1e-12, JSON.stringify(spun));
  assert.deepEqual(reportOf([1], [0], 'picture'), {});
});

test('a loop effect that reads the flow gets it where it runs: none in the carry pass, the fluid’s in the light pass', () => {
  const carry = loopFunction(8);
  assert.ok(carry.includes('vec2 fx_flow(vec2 p) { return vec2(0.0); }') && carry.indexOf('fx_flow(vec2 p)') < carry.indexOf('vec2 fx_stream('), 'defined before Stream uses it');
  const light = loopFunction(8, 'vec2 fx_flow(vec2 p) { return p; }');
  assert.ok(light.startsWith('vec2 fx_flow(vec2 p) { return p; }'));
});

test('the beat: where in it and its bar, how long since, how strong; none is long ago; only the picture reads it', async () => {
  const { beatOf, effectsNow } = await import('../src/lib/visuals/effects.ts');
  const [phase, since, strength, bar] = beatOf({ index: 6, since: 0.25, length: 0.5, strength: 0.8 });
  assert.ok(Math.abs(phase - 0.5) < 1e-9 && since === 0.25 && strength === 0.8 && Math.abs(bar - (2 + 0.5) / 4) < 1e-9);
  assert.deepEqual(beatOf(null), [0, 1000, 0, 0]);
  assert.deepEqual(effectsNow({ fx1: 16, fx1Amount: 1, fx2: 0, fx2Amount: 0, fx3: 0, fx3Amount: 0, fxLoop: 0, fxLoopAmount: 0 }, 1, 0).beat, [0, 1000, 0, 0]);
  for (const e of EFFECTS) if (e.glsl.includes('fx_beat(')) assert.ok(!e.slots.includes('loop'), `${e.name}: the loop has no beat`);
  assert.ok(pictureShader([16])!.includes('vec4 fx_beat()'));
});

test('three engines: every effect in exactly one, the loop’s its shapes; a setting counts its engine’s sub-effects', async () => {
  const { ENGINES, ENGINE_EFFECTS, LOOP_EFFECTS, engineEffect, loopEffect, effectsNow } = await import('../src/lib/visuals/effects.ts');
  assert.deepEqual(ENGINES.map((e) => e.label), ['Kaleidoscope', 'EYEDO', 'Hydra']);
  const seen = new Map<number, string>();
  for (const { key } of ENGINES)
    for (const e of ENGINE_EFFECTS[key]) {
      assert.ok(e, `${key}: an id with no effect`);
      assert.equal(e.engine, key, `${e.name} in ${key}`);
      assert.ok(!seen.has(e.id), `${e.name} in ${seen.get(e.id)} and ${key}`);
      seen.set(e.id, key);
    }
  assert.equal(seen.size, EFFECTS.length, 'every effect in an engine');
  for (const e of LOOP_EFFECTS) assert.ok(e && e.kind === 'shape' && e.slots.includes('loop'), `${e?.name}: for the loop`);
  assert.equal(LOOP_EFFECTS.length, EFFECTS.filter((e) => e.slots.includes('loop')).length, 'every loop shape offered');
  assert.equal(engineEffect('kaleidoscope', 1), 1);
  assert.equal(engineEffect('hydra', 0), 0);
  assert.equal(engineEffect('eyedo', 99), 0, 'past its last: none');
  assert.equal(loopEffect(1), LOOP_EFFECTS[0].id);
  const fx = effectsNow({ fx1: 2, fx1Amount: 0.5, fx2: 1, fx2Amount: 1, fx3: 0, fx3Amount: 1, fxLoop: 0, fxLoopAmount: 0 }, 1, 0);
  assert.deepEqual(fx.ids, [ENGINE_EFFECTS.kaleidoscope[1].id, ENGINE_EFFECTS.eyedo[0].id, 0]);
});

test('each effect has at most two knobs, each a label and its text; the rows a chosen one shows', () => {
  for (const e of EFFECTS) {
    assert.ok(e.knobs.length <= 2, `${e.name}: two knobs at most`);
    for (const k of e.knobs) assert.ok(k.label && typeof k.text(0.5) === 'string' && k.text(0) !== undefined && k.text(1) !== undefined, `${e.name}: ${k.label}`);
  }
  const kaleid = EFFECTS.find((e) => e.name === 'kaleid')!;
  assert.deepEqual(kaleid.knobs.map((k) => k.label), ['Sides', 'Spin']);
  assert.equal(kaleid.knobs[0].text(0.5), '7');
  assert.equal(kaleid.knobs[1].text(0.5), 'None');
});

test('the governor reaches the effects: the swing always, and the lever of each meter an effect counts toward', async () => {
  const { heldBack, effectsNow, settingsReport } = await import('../src/lib/visuals/effects.ts');
  const open = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
  // Posterize counts toward no meter: only the flashes' swing holds it.
  assert.equal(heldBack(2, 1, [0.5, 0.5], { ...open, pace: 0.3, evolution: 4 }), 1);
  assert.equal(heldBack(2, 1, [0.5, 0.5], { ...open, swing: 0.5 }), 0.5);
  // Kaleid spinning counts as motion and shape churn: the pace and the evolving both hold it.
  assert.ok(Math.abs(heldBack(1, 1, [0.5, 1], { ...open, pace: 0.5, evolution: 2 }) - 0.25) < 1e-12);
  // Not spinning, only shape churn: the pace does not.
  assert.ok(Math.abs(heldBack(1, 1, [0.5, 0.5], { ...open, pace: 0.5, evolution: 2 }) - 0.5) < 1e-12);
  // Pulse counts as zoom pumping: the size holds it.
  assert.ok(Math.abs(heldBack(16, 1, [0.5, 0.5], { ...open, size: 0.4 }) - 0.4) < 1e-12);
  const how = { fx1: 1, fx1Amount: 1, fx2: 1, fx2Amount: 1, fx3: 0, fx3Amount: 0, fxLoop: 0, fxLoopAmount: 0, fx1K1: 0.5, fx1K2: 1, fx2K1: 0.5, fx2K2: 0.5, fx3K1: 0.5, fx3K2: 0.5, fxLoopK1: 0.5, fxLoopK2: 0.5 };
  const held = effectsNow(how, 1, 0, null, { ...open, pace: 0.5 });
  assert.deepEqual(held.amounts.map((a) => +a.toFixed(3)), [0.5, 1, 0], 'Kaleid spinning held by the pace; Pulse not');
  // What they count is counted as held: the governor and the precheck read the same.
  assert.ok(settingsReport(how, 1, { ...open, pace: 0.5 }).motion! < settingsReport(how, 1).motion!);
});
