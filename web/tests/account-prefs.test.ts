// The preferences on the account (lib/accountPrefsRule.ts, decision 0101 point 14): per preference the latest change
// wins, the account's when as late; a copy from before 0.16 is its account's, sent where the server holds none;
// another account's copy is let go, never sent; one value reads the same however its keys were put together.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { canon, meet, signedIn } = await import('../src/lib/accountPrefsRule.ts');

const KEYS = ['pinned', 'filters'] as const;

test('the later change wins, the account\'s when as late', () => {
  assert.equal(meet({ changed: 9 }, 5), 'take');
  assert.equal(meet({ changed: 5 }, 9), 'send');
  assert.equal(meet({ changed: 5 }, 5), 'take');
});

test('the server holding none: this device sends its own, a default nothing', () => {
  assert.equal(meet(undefined, 0), 'send', 'a copy from before 0.16');
  assert.equal(meet(undefined, 7), 'send');
  assert.equal(meet(undefined, undefined), null);
});

test('a default here takes whatever the account holds', () => {
  assert.equal(meet({ changed: 0 }, undefined), 'take');
});

test('a copy from before 0.16 becomes its signed-in account\'s, each preference to be sent', () => {
  const { copy, fresh } = signedIn({ account: null, changed: { pinned: 4 } }, 'ann', KEYS);
  assert.equal(fresh, false);
  assert.deepEqual(copy, { account: 'ann', changed: { pinned: 4, filters: 0 } });
});

test("another account's copy is let go: its preferences to their defaults, nothing of it sent", () => {
  const { copy, fresh } = signedIn({ account: 'ann', changed: { pinned: 4, filters: 0 } }, 'bo', KEYS);
  assert.equal(fresh, true);
  assert.deepEqual(copy, { account: 'bo', changed: {} });
  assert.equal(meet(undefined, copy.changed.pinned), null, 'the server holding none, nothing goes');
});

test('the same account: the copy as it is', () => {
  const was = { account: 'ann', changed: { pinned: 4 } };
  assert.equal(signedIn(was, 'ann', KEYS).copy, was);
});

test('one value reads the same whatever order its keys came in', () => {
  assert.equal(canon({ b: 1, a: { d: [2, { y: 1, x: 0 }], c: null } }), canon({ a: { c: null, d: [2, { x: 0, y: 1 }] }, b: 1 }));
  assert.notEqual(canon([1, 2]), canon([2, 1]), 'a list keeps its order');
  assert.equal(canon(undefined), 'null');
});
