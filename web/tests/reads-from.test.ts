// Where reads come from (lib/deviceIndexRule): the one decision, and how Cratarr's reach settles before it moves them.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { answered, lookedAgain, LOOK_AGAIN_MS, notReached, readsFrom } = await import('../src/lib/deviceIndexRule.ts');

test('with Cratarr answering, reads come from the server', () => {
  assert.deepEqual(readsFrom({ unreachable: false, noNetwork: false }), { from: 'server', why: 'answers' });
});

test('with Cratarr out of reach, settled, they come from the device, and it says why', () => {
  assert.deepEqual(readsFrom({ unreachable: true, noNetwork: false }), { from: 'device', why: 'unreachable' });
});

test('a device with no network reads itself with nothing to settle', () => {
  assert.deepEqual(readsFrom({ unreachable: false, noNetwork: true }), { from: 'device', why: 'no-network' });
  assert.deepEqual(readsFrom({ unreachable: true, noNetwork: true }), { from: 'device', why: 'no-network' });
});

test('one failed request settles nothing: it names when to look again', () => {
  assert.deepEqual(notReached(answered, 1000), { failed: 1, lookAt: 1000 + LOOK_AGAIN_MS });
});

test('more failed requests before the second look do not put it off', () => {
  const first = notReached(answered, 1000);
  assert.deepEqual(notReached(first, 5000), first);
});

test('a second look that finds nothing settles it, and more failed requests leave it settled', () => {
  const settled = lookedAgain(notReached(answered, 1000), false);
  assert.deepEqual(settled, { failed: 2, lookAt: null });
  assert.deepEqual(notReached(settled, 9000), settled);
});

test('a second look that finds Cratarr was a blip', () => {
  assert.deepEqual(lookedAgain(notReached(answered, 1000), true), answered);
});

test('a look that comes back after Cratarr answered some other way says nothing', () => {
  assert.deepEqual(lookedAgain(answered, false), answered);
  assert.deepEqual(lookedAgain(answered, true), answered);
});

test('settled, a late look does not undo it', () => {
  const settled = lookedAgain(notReached(answered, 1000), false);
  assert.deepEqual(lookedAgain(settled, true), settled);
});
