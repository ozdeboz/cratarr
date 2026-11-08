// Whose the device's stores are (lib/account.ts, decision 0101): an account's key as the page's stores name it, the
// same as the app's (android Who.kt): `@name`, `@` with sign-in off, `-` for nobody; nobody learnt yet is nobody.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { accountKey, accountForgotten, accountLearnt, perAccount, whenAccount, whoOf } = await import('../src/lib/account.ts');

test('an account, the one user, nobody: as the app names them', () => {
  assert.equal(whoOf('ann'), '@ann');
  assert.equal(whoOf(''), '@');
  assert.equal(whoOf(null), '-');
});

test('a store is named for the account signed in, and told when another is', () => {
  assert.equal(accountKey(), null);
  assert.equal(perAccount('cratarr-shelf'), 'cratarr-shelf-');
  let told = 0;
  const f = () => told++;
  whenAccount.add(f);
  accountLearnt('ann');
  assert.equal(perAccount('cratarr-shelf'), 'cratarr-shelf@ann');
  accountLearnt('ann');
  assert.equal(told, 1, 'the same one again is no change');
  accountLearnt(null);
  assert.equal(perAccount('cratarr-shelf'), 'cratarr-shelf@');
  accountForgotten();
  assert.equal(perAccount('cratarr-shelf'), 'cratarr-shelf-', 'signed out: nobody, never the one before');
  assert.equal(told, 3);
  whenAccount.delete(f);
});
