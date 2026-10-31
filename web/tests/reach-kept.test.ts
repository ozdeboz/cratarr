// An answer from the service worker's last copy (0.12.3; the owner: Home flickered): soon after a live answer it is a
// slow answer, not out of reach; long after, out of reach.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { keptMeansAway, KEPT_GRACE_MS } = await import('../src/lib/reach.svelte.ts');

test('a slow answer soon after a live one is not out of reach; long after, it is', () => {
  const now = 1_000_000;
  assert.equal(keptMeansAway(now - 1_000, now), false);
  assert.equal(keptMeansAway(now - KEPT_GRACE_MS, now), false);
  assert.equal(keptMeansAway(now - KEPT_GRACE_MS - 1, now), true);
  assert.equal(keptMeansAway(0, now), true);
});
