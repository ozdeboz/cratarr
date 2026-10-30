// Noise and Mix's Subtract (decision 0057 §4, the owner 2026-10-20): Noise draws the same for the same module and time
// on every device, grain at its finest; Subtract is the picture less the other, never below black, and no saved mode
// changed its number: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { NOISE, noiseNow, noiseSeed, noiseStep } = await import('../src/lib/visuals/sources/noise.ts');
const { defaults } = await import('../src/lib/visuals/sources/knob.ts');
const { MIX_MODES } = await import('../src/lib/visuals/ports.ts');
const { BLENDS } = await import('../src/lib/visuals/patch.ts');
const { mixShader } = await import('../src/lib/visuals/compose.ts');
const { categoryOf } = await import('../src/lib/visuals/rack.ts');

test('Noise: the same for the same module and time, another module elsewhere in it; a source, light only', () => {
  assert.deepEqual(noiseSeed('noise-1'), noiseSeed('noise-1'));
  assert.notDeepEqual(noiseSeed('noise-1'), noiseSeed('noise-2'));
  for (const v of noiseSeed('grain')) assert.ok(Number.isFinite(v) && Math.abs(v) <= 100);
  const k = defaults(NOISE);
  assert.deepEqual(noiseNow(k, 12.5, 1080), noiseNow(k, 12.5, 1080));
  assert.notEqual(noiseNow(k, 12.5, 1080).z, noiseNow(k, 12.6, 1080).z);
  assert.equal(noiseNow({ ...k, speed: 0 }, 12.5, 1080).z, 0, 'still at speed 0');
  assert.equal(categoryOf(NOISE), 'sources');
  assert.deepEqual(NOISE.knobs.map((x) => x.id), ['amount', 'scale', 'detail', 'speed', 'contrast']);
  assert.ok(!NOISE.knobs.some((x) => /hue|saturation/.test(x.id)), 'monochrome');
});

test('Noise at its finest scale, moving, is grain: a new pattern every frame; coarse, it drifts', () => {
  const k = defaults(NOISE);
  const frame = 1 / 60;
  const fine = noiseNow({ ...k, scale: 0 }, 10, 1080);
  assert.ok(fine.cells > 600, 'a cell about a pixel and a half');
  assert.ok(noiseNow({ ...k, scale: 0 }, 10 + frame, 1080).z - fine.z >= 1, 'a unit of z (another pattern) each frame');
  const coarse = noiseNow({ ...k, scale: 1 }, 10, 1080);
  assert.ok(coarse.cells < 1);
  assert.ok(noiseNow({ ...k, scale: 1 }, 10 + frame, 1080).z - coarse.z < 0.01);
  // Its octaves 1 to 6; contrast 50% as it is.
  assert.equal(noiseNow({ ...k, detail: 9 }, 0, 100).octaves, 6);
  assert.equal(noiseNow({ ...k, contrast: 0.5 }, 0, 100).gain, 1);
  // Wrapped, so the shader's float keeps its fractions after hours.
  assert.ok(noiseNow({ ...k, scale: 0, speed: 1 }, 36000, 1080).z < 4096);
});

test('Subtract appended: every saved mode keeps its number', () => {
  assert.deepEqual(MIX_MODES, ['add', 'screen', 'over', 'multiply', 'window', 'lighten', 'subtract']);
  // A layer's blends: Mix's first modes, at the same places.
  assert.deepEqual(BLENDS, MIX_MODES.slice(0, BLENDS.length));
  const s = mixShader(2);
  // The shader's modes by those numbers: today's as they were, Lighten and Subtract after them.
  for (const i of [0, 1]) {
    assert.ok(s.includes(`if (uMode${i} == 1) c = c + b.rgb - c * b.rgb;`));
    assert.ok(s.includes(`else if (uMode${i} == 2) c = b.rgb + c * (1.0 - b.a);`));
    assert.ok(s.includes(`else if (uMode${i} == 3) c = c * mix(vec3(1.0), b.rgb / max(b.a, 1e-4), b.a);`));
    assert.ok(s.includes(`else if (uMode${i} == 4) c = c * mix(1.0 - uAmount${i}, 1.0, cover);`));
    assert.ok(s.includes(`else if (uMode${i} == 5) c = max(c, b.rgb);`));
    assert.ok(s.includes(`else if (uMode${i} == 6) c = max(vec3(0.0), c - b.rgb);`));
  }
});

test('Subtract: the picture less the other (by its amount), never below black; a picture less itself is black', () => {
  // The shader's Subtract (above), per channel, after `b *= amount`.
  const subtract = (below: number, above: number, amount: number) => Math.max(0, below - above * amount);
  for (const c of [0, 0.2, 0.75, 1]) {
    assert.equal(subtract(c, c, 1), 0);
    assert.equal(subtract(c, 0, 1), c);
    assert.equal(subtract(c, c, 0), c);
  }
  assert.equal(subtract(0.2, 0.9, 1), 0);
});

test('its time runs on a frame at a time: a change of speed or scale goes on from where it is, no jump', () => {
  const k = Object.fromEntries(NOISE.knobs.map((x) => [x.id, x.default]));
  let z = 0;
  for (let i = 0; i < 600; i++) z = noiseStep(z, k, 1 / 60, 1080);
  const before = z;
  const changed = { ...k, speed: 1, scale: 0.1 };
  const faster = noiseStep(z, changed, 1 / 60, 1080);
  const rate = 2 * noiseNow(changed, 0, 1080).cells;
  assert.ok(Math.abs(faster - before - rate / 60) < 1e-9, 'one frame on at the new rate');
  // As it was (the track's time times the speed and scale): a jump the further the later.
  assert.ok(Math.abs(noiseNow(changed, 10, 1080).z - noiseNow(k, 10, 1080).z) > 100 * (rate / 60), 'where the old z would have jumped');
  assert.ok(Math.abs(noiseStep(z, { ...k, speed: 0 }, 1 / 60, 1080) - z) < 1e-12, 'still at speed 0');
});
