// A stream asked ahead (lib/player/armSlot): asked anew, the one before let go; taken, the taker's; dropped, gone; one
// that gave up lets go of itself only while it is the one held.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { armSlot } = await import('../src/lib/player/armSlot.ts');

test('a wind to a point asked again: the stream before let go, the new one held', () => {
  const gone: string[] = [];
  const s = armSlot<string>((a) => gone.push(a));
  s.arm('a');
  s.arm('b');
  assert.deepEqual(gone, ['a']);
  assert.equal(s.now, 'b');
});

test("a hold's stream taken at the hold: the taker's, nothing let go", () => {
  const gone: string[] = [];
  const s = armSlot<string>((a) => gone.push(a));
  s.arm('h');
  assert.equal(s.take(), 'h');
  assert.equal(s.now, null);
  assert.deepEqual(gone, []);
});

test('a tap after all: its stream dropped; one giving up lets go only of itself', () => {
  const gone: string[] = [];
  const s = armSlot<string>((a) => gone.push(a));
  s.arm('t');
  s.drop();
  assert.deepEqual(gone, ['t']);
  s.arm('x');
  s.release('older');
  assert.equal(s.now, 'x', 'a stream given up that is no longer held changes nothing');
  s.release('x');
  assert.equal(s.now, null);
});
