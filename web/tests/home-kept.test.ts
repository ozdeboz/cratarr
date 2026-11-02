// Home's kept moment (lib/homeKept; the owner, 2026-11-01): shown again only within half an hour and in the same part
// of the day.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { momentFresh, partOfDay } = await import('../src/lib/homeKept.svelte.ts');

test('the parts of the day', () => {
  const at = (h: number) => partOfDay(new Date(2026, 10, 1, h, 30));
  assert.deepEqual([at(4), at(5), at(11), at(12), at(17), at(18), at(22), at(23)], ['night', 'morning', 'morning', 'day', 'day', 'evening', 'evening', 'night']);
});

test('a kept moment holds for half an hour in the same part of the day', () => {
  const asked = new Date(2026, 10, 1, 14, 0).getTime();
  const k = { moment: { name: 'x' }, momentAt: asked, momentPart: 'day' };
  assert.equal(momentFresh(k, asked + 10 * 60_000), true);
  assert.equal(momentFresh(k, asked + 31 * 60_000), false, 'too old');
  const evening = new Date(2026, 10, 1, 17, 50).getTime();
  assert.equal(momentFresh({ ...k, momentAt: evening }, new Date(2026, 10, 1, 18, 5).getTime()), false, 'the evening began');
  assert.equal(momentFresh({ ...k, moment: undefined }, asked), false, 'none kept');
});
