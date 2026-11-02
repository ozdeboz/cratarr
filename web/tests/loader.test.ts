// One loading rule (0.13; the owner, 2026-10-31: "Keep, then spinner at 300 ms"): what was shown stays while its
// replacement is on its way, the loader takes its place at LOADER_AFTER_MS, and the answer replaces either at any time.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { LOADER_AFTER_MS, waitShows } = await import('../src/lib/loader.ts');

test('one threshold: 300 ms', () => {
  assert.equal(LOADER_AFTER_MS, 300);
});

test('before the threshold what was shown is kept, never blanked', () => {
  assert.equal(waitShows(1000, 1000), 'kept');
  assert.equal(waitShows(1000, 1000 + LOADER_AFTER_MS - 1), 'kept');
});

test('at and after the threshold the loader takes its place', () => {
  assert.equal(waitShows(1000, 1000 + LOADER_AFTER_MS), 'loader');
  assert.equal(waitShows(1000, 1000 + 10 * LOADER_AFTER_MS), 'loader');
});

test('the answer replaces either, at any time', () => {
  // Nothing waited for: what is there is the answer, however long the wait was.
  for (const now of [0, 100, LOADER_AFTER_MS, 60_000]) assert.equal(waitShows(null, now), 'answer');
});

test('a new wait (the words changed) counts again from its own start', () => {
  const first = 1000;
  const again = first + LOADER_AFTER_MS - 50; // typed on before the loader was due
  assert.equal(waitShows(again, first + LOADER_AFTER_MS + 10), 'kept');
  assert.equal(waitShows(again, again + LOADER_AFTER_MS), 'loader');
});
