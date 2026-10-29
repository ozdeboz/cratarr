// One shuffle for every way of playing (lib/shuffle; the architecture audit, 2026-10-29: two biased
// `sort(() => Math.random() - 0.5)` beside two copies of Fisher–Yates).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { shuffled } = await import('../src/lib/shuffle.ts');

test('a shuffle keeps every track once and leaves the list as it was', () => {
  const tracks = Array.from({ length: 50 }, (_, i) => i);
  const out = shuffled(tracks);
  assert.deepEqual([...out].sort((a, b) => a - b), tracks);
  assert.deepEqual(tracks, Array.from({ length: 50 }, (_, i) => i));
});

test('every order is as likely', () => {
  const runs = 6000;
  const seen = new Map<string, number>();
  for (let i = 0; i < runs; i++) {
    const k = shuffled(['a', 'b', 'c']).join('');
    seen.set(k, (seen.get(k) ?? 0) + 1);
  }
  assert.equal(seen.size, 6);
  // About 1000 each (sigma about 29): far outside this only if some orders are favoured.
  for (const [order, n] of seen) assert.ok(n > 800 && n < 1200, `${order}: ${n}`);
});
