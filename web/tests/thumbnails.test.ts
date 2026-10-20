// The graph's thumbnails (thumbnails.ts, decision 0057): which modules a frame leaves a picture of their own: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { outputsOf, reduceMax, thumbSize } = await import('../src/lib/visuals/thumbnails.ts');

test('a module keeps its picture until its target is drawn into again (the pool gives it out once read)', () => {
  const out = outputsOf([['show', 'A'], ['glow', 'B'], ['tint', 'A'], ['mix', 'C']]);
  assert.deepEqual([...out], [['glow', 'B'], ['tint', 'A'], ['mix', 'C']]);
});

test('a module at rest shows what it passes on; one fused into a pass (no draw) has none', () => {
  const alias = new Map([['glow', { module: 'tint' }], ['tint', { module: 'show' }], ['keep', { module: 'gone' }]]);
  const out = outputsOf([['show', 'A'], ['mix', 'B']], alias);
  assert.equal(out.get('glow'), 'A');
  assert.equal(out.get('tint'), 'A');
  assert.ok(!out.has('keep') && !out.has('warp'));
});

test('a thumbnail is two quarters of each side (about a sixteenth), a pixel at least', () => {
  assert.deepEqual(thumbSize(1920, 1080), [120, 68]);
  assert.deepEqual(thumbSize(8, 4), [1, 1]);
});

test('reduced by the most of each block: a one-pixel line stays as bright, wherever it falls', () => {
  const [W, H] = [64, 37];
  for (const x of [0, 13, 31, 63]) {
    const px = new Float32Array(W * H * 4);
    for (let y = 0; y < H; y++) px.set([0.9, 0.5, 0.1, 1], (y * W + x) * 4);
    const a = reduceMax(px, W, H);
    const b = reduceMax(a.px, a.width, a.height);
    assert.deepEqual([b.width, b.height], thumbSize(W, H));
    for (let y = 0; y < b.height; y++) {
      const row = Array.from({ length: b.width }, (_, i) => b.px[(y * b.width + i) * 4]);
      assert.equal(row.filter((v) => Math.abs(v - 0.9) < 1e-6).length, 1, `line at ${x}, row ${y}`);
      assert.equal(row.filter((v) => v === 0).length, b.width - 1);
    }
  }
});
