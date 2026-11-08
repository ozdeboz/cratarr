// The deck's words when the tape cannot do what it was turned to (lib/tape/words.ts): the reason, the way it was to go;
// nothing for a reason it does not know.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { refusedText } = await import('../src/lib/tape/words.ts');

test('backwards, and a wind forwards, each with its reason', () => {
  assert.equal(refusedText('unreachable', 'back'), "Can't play backwards: Cratarr isn't reachable");
  assert.equal(refusedText('late', 'back'), "Can't play backwards: its sound came too late");
  assert.equal(refusedText('late', 'forward'), "Can't wind: its sound came too late");
  assert.equal(refusedText('unreachable', undefined), "Can't wind: Cratarr isn't reachable");
});

test('a reason it does not know says nothing', () => {
  assert.equal(refusedText('gone', 'back'), null);
  assert.equal(refusedText(undefined, 'back'), null);
});
