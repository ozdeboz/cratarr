// The preferences on the account (lib/accountPrefsRule.ts, decision 0101 point 14): per preference the latest change
// wins, the account's when as late; what changed here goes where the server holds none, a default never; one value reads
// the same however its keys were put together. Two accounts on one device: tests/isolated/two-accounts.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { canon, meet } = await import('../src/lib/accountPrefsRule.ts');

test('the later change wins, the account\'s when as late', () => {
  assert.equal(meet({ changed: 9 }, 5), 'take');
  assert.equal(meet({ changed: 5 }, 9), 'send');
  assert.equal(meet({ changed: 5 }, 5), 'take');
});

test('the server holding none: this device sends its own, a default nothing', () => {
  assert.equal(meet(undefined, 7), 'send');
  assert.equal(meet(undefined, undefined), null);
});

test('a default here takes whatever the account holds', () => {
  assert.equal(meet({ changed: 0 }, undefined), 'take');
});

test('one value reads the same whatever order its keys came in', () => {
  assert.equal(canon({ b: 1, a: { d: [2, { y: 1, x: 0 }], c: null } }), canon({ a: { c: null, d: [2, { x: 0, y: 1 }] }, b: 1 }));
  assert.notEqual(canon([1, 2]), canon([2, 1]), 'a list keeps its order');
  assert.equal(canon(undefined), 'null');
});
