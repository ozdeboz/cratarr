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
  assert.deepEqual(chainOf([1, 2, 7], 'loop').map((e) => e.name), ['kaleid']);
});

test('the loop: a shape on the buffer’s coordinates, else the identity; reports add up by amount', () => {
  assert.ok(loopFunction(1).includes('p = fx_kaleid(p, a, t);'));
  assert.ok(loopFunction(2).includes('return uv;'), 'a colour is not for the loop');
  const r = reportOf([1, 1], [0.5, 1], 'picture');
  assert.ok(Math.abs(r.motion! - 0.075) < 1e-12 && Math.abs(r.shapeChurn! - 0.15) < 1e-12, JSON.stringify(r));
  assert.deepEqual(reportOf([1], [0], 'picture'), {});
});
