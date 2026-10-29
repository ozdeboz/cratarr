// A filter row asks nothing until it says what (lib/filterRow `complete`; the owner, 2026-10-29: a new Type row with
// no value chosen emptied the Library, "0 of 2,972"): a choice of none, but a playlist's, and a range open at both ends
// filter nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { complete } = await import('../src/lib/filterRow.ts');

const row = (axis: string, value: object) => ({ axis, not: false, value }) as Parameters<typeof complete>[0];

test('a row with nothing chosen asks nothing yet', () => {
  assert.equal(complete(row('type', { kind: 'any_of', values: [] })), false);
  assert.equal(complete(row('year', { kind: 'range', min: null, max: null })), false);
  assert.equal(complete(row('sound', { kind: 'like', of: [], count: 20 })), false);
});

test('a row that says what it asks filters', () => {
  assert.equal(complete(row('type', { kind: 'any_of', values: ['Album'] })), true);
  assert.equal(complete(row('year', { kind: 'range', min: 1990, max: null })), true);
  assert.equal(complete(row('year', { kind: 'range', min: null, max: 2000 })), true);
  assert.equal(complete(row('favourite', { kind: 'yes' })), true);
});

test('a playlist row with none chosen is "in any playlist"', () => {
  assert.equal(complete(row('playlist', { kind: 'any_of', values: [] })), true);
});
