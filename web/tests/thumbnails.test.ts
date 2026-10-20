// The graph's thumbnails (thumbnails.ts, decision 0057): which modules a frame leaves a picture of their own: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { outputsOf, thumbSize } = await import('../src/lib/visuals/thumbnails.ts');

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

test('a thumbnail is a sixteenth of each side, a pixel at least', () => {
  assert.deepEqual(thumbSize(1920, 1080), [120, 68]);
  assert.deepEqual(thumbSize(8, 4), [1, 1]);
});
