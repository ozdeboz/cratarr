// The effects stage's chains (effects.ts): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chainOf, EFFECTS, loopFunction, pictureShader, reportOf } from '../src/lib/visuals/effects.ts';

test('every effect is one function of its kind, named after it, and its id is its own', () => {
  const ids = new Set<number>();
  for (const e of EFFECTS) {
    assert.ok(!ids.has(e.id), `id ${e.id} once`);
    ids.add(e.id);
    const head = { shape: `vec2 fx_${e.name}(vec2 p, float a, float t)`, colour: `vec3 fx_${e.name}(vec3 c, vec2 p, float a, float t)`, echo: `vec3 fx_${e.name}(vec3 c, vec2 p, float a, float t, sampler2D prev)` }[e.kind];
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
  assert.ok(s.indexOf('p = fx_kaleid(p, uFxA[1], uFxT);') < read, 'the shape first, by its place');
  assert.ok(s.indexOf('c = fx_posterize(c, p, uFxA[0], uFxT);') > read, 'the colour after');
  assert.equal(s.split('vec2 fx_kaleid(').length, 2, 'each function once');
  assert.deepEqual(chainOf([1, 2, 999], 'loop').map((e) => e.name), ['kaleid'], 'a colour and one not known: not for the loop');
});

test('the loop: a shape on the buffer’s coordinates, else the identity; reports add up by amount', () => {
  assert.ok(loopFunction(1).includes('p = fx_kaleid(p, a, t);'));
  assert.ok(loopFunction(2).includes('return uv;'), 'a colour is not for the loop');
  const r = reportOf([1, 1], [0.5, 1], 'picture');
  assert.ok(Math.abs(r.motion! - 0.075) < 1e-12 && Math.abs(r.shapeChurn! - 0.15) < 1e-12, JSON.stringify(r));
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
