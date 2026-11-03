// A range condition's scale where its tracks are (lib/rangeScale; the owner, 2026-11-02: BPM 30 to 999).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { trimScale } = await import('../src/lib/rangeScale.ts');

test("a few outliers' tail is cut off, the tracks' own span kept", () => {
  // 30..990 in ten 96-wide bins: nearly every track in the first two, one at the far end.
  const t = trimScale([400, 590, 9, 0, 0, 0, 0, 0, 0, 1], 30, 990);
  assert.deepEqual([t.min, t.max, t.bins], [30, 318, [400, 590, 9]]);
});

test('nothing thin at either end: the whole scale', () => {
  assert.deepEqual(trimScale([10, 20, 30, 20, 10], 0, 50), { bins: [10, 20, 30, 20, 10], min: 0, max: 50 });
  assert.deepEqual(trimScale([], 0, 1), { bins: [], min: 0, max: 1 });
});
