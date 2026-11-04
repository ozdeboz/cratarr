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

test('a player that says its speed is counted on at it, and from where it said a new one (no guessing)', () => {
  // A Vinyl ramp on another player, at half speed as it says: two seconds on is one second of the track, not two.
  const a = heardAt(null, { id: 't', pos: 10, paused: false, speed: 0.5 }, 1000);
  assert.equal(placeOf(a, 3000), 11);
  // It says 1× again: on from that report at 1×; the time before it is not counted again at the new speed.
  const b = heardAt(a, { id: 't', pos: 11.2, paused: false, speed: 1 }, 3400);
  assert.deepEqual(b, { id: 't', pos: 11.2, at: 3400, paused: false, speed: 1 });
  assert.equal(placeOf(b, 4400), 12.2);
});
