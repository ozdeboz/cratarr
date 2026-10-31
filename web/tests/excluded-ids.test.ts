// A track's "Exclude this track everywhere" switch (the owner, 2026-10-31; lib/excludedIds): it shows only what its
// source's list says, and each change keeps the list true both ways.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { excludedIn, withExcluded } = await import('../src/lib/excludedIds.ts');

test('a track reads as excluded only from its own source, and unknown while that list is not read', () => {
  const lists = { cratarr: ['a', 'b'] };
  assert.equal(excludedIn(lists, 'cratarr', 'a'), true);
  assert.equal(excludedIn(lists, 'cratarr', 'c'), false);
  assert.equal(excludedIn(lists, 'other', 'a'), null);
});

test('excluding and taking back change the list once, each way', () => {
  const lists = { cratarr: ['a'] };
  const out = withExcluded(lists, 'cratarr', ['b'], true);
  assert.deepEqual(out.cratarr, ['a', 'b']);
  assert.deepEqual(withExcluded(out, 'cratarr', ['b'], true).cratarr, ['a', 'b']);
  assert.deepEqual(withExcluded(out, 'cratarr', ['a', 'b'], false).cratarr, []);
  assert.deepEqual(lists.cratarr, ['a']);
});

test('an unread list stays unread: no switch is shown on a guess', () => {
  const lists = { cratarr: ['a'] };
  assert.equal(withExcluded(lists, 'other', ['x'], true), lists);
  assert.equal(excludedIn(withExcluded(lists, 'other', ['x'], true), 'other', 'x'), null);
});
