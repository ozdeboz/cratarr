// The capability list's rule (lib/capabilityRule.ts, decision 0085): a name listed can, one not listed cannot; a
// server before 0.11 (a 404) or no answer yet decides nothing, so each place keeps its old check; a failed ask keeps
// what was known; a kept copy reads back only as one of those.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { NO_CAPABILITIES, answered, canBy, keptState } = await import('../src/lib/capabilityRule.ts');

test('a listed name can, an unlisted one cannot', () => {
  const s = answered(NO_CAPABILITIES, { names: ['track_search', 'batches'] });
  assert.equal(canBy(s, 'track_search'), true);
  assert.equal(canBy(s, 'track_intake'), false);
});

test('no answer yet decides nothing', () => {
  assert.equal(canBy(NO_CAPABILITIES, 'batches'), null);
});

test('a server before 0.11 decides nothing, whatever was known before', () => {
  const s = answered(answered(NO_CAPABILITIES, { names: ['batches'] }), 'older');
  assert.deepEqual(s, { names: null, older: true });
  assert.equal(canBy(s, 'batches'), null);
});

test('a failed ask keeps what was known', () => {
  const known = answered(NO_CAPABILITIES, { names: ['follow'] });
  assert.equal(answered(known, 'failed'), known);
  assert.equal(answered(NO_CAPABILITIES, 'failed'), NO_CAPABILITIES);
});

test('an empty list cannot anything (it is an answer, not none known)', () => {
  assert.equal(canBy(answered(NO_CAPABILITIES, { names: [] }), 'follow'), false);
});

test('a newer answer replaces the list, and the answer is copied', () => {
  const names = ['a', 'b'];
  const s = answered(answered(NO_CAPABILITIES, { names: ['c'] }), { names });
  names.push('c');
  assert.equal(canBy(s, 'c'), false);
  assert.equal(canBy(s, 'a'), true);
});

test('a kept copy reads back', () => {
  assert.deepEqual(keptState({ names: ['x'], older: false }), { names: ['x'], older: false });
  assert.deepEqual(keptState({ names: null, older: true }), { names: null, older: true });
  assert.deepEqual(keptState(JSON.parse(JSON.stringify(answered(NO_CAPABILITIES, { names: ['y'] })))), { names: ['y'], older: false });
});

test('anything else kept reads as none known', () => {
  for (const v of [null, undefined, 'x', 3, [], {}, { names: 'x' }, { names: [1, 'a'] }, { older: 'yes' }])
    assert.deepEqual(keptState(v), NO_CAPABILITIES);
});
