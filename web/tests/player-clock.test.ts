// The one clock (lib/player/clock): another player's place between its reports, run on from an anchor; a report far
// off taken as it is, a small one eased in.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { heardAt, placeOf, JUMP } = await import('../src/lib/player/clock.ts');

test('runs on from the anchor while it plays, stands while paused, no further than the length', () => {
  const a = { id: 't', pos: 10, at: 1000, paused: false };
  assert.equal(placeOf(a, 3000), 12);
  assert.equal(placeOf({ ...a, paused: true }, 3000), 10);
  assert.equal(placeOf(a, 1_000_000, 60), 60);
  assert.equal(placeOf(null, 5000), 0);
});

test('a report far off, another track, a pause or a rewind is taken as it is', () => {
  const a = { id: 't', pos: 10, at: 0, paused: false };
  assert.deepEqual(heardAt(a, { id: 't', pos: 40, paused: false }, 1000), { id: 't', pos: 40, at: 1000, paused: false }, 'a seek');
  assert.deepEqual(heardAt(a, { id: 'u', pos: 0, paused: false }, 1000), { id: 'u', pos: 0, at: 1000, paused: false });
  assert.equal(heardAt(a, { id: 't', pos: 11, paused: true }, 1000).paused, true);
  assert.deepEqual(heardAt(a, { id: 't', pos: 9, paused: false, rewinding: true }, 1000), { id: 't', pos: 9, at: 1000, paused: true });
});

test('a small difference is eased in, half at a time, never a jump', () => {
  const a = { id: 't', pos: 10, at: 0, paused: false };
  const b = heardAt(a, { id: 't', pos: 11.6, paused: false }, 1000); // predicted 11, heard 11.6
  assert.ok(Math.abs(placeOf(b, 1000) - 11.3) < 1e-9);
  assert.ok(JUMP > 0.6);
  assert.equal(heardAt(a, { id: 't', pos: 11, paused: false }, 1000), a, 'as predicted: the anchor stays');
});
