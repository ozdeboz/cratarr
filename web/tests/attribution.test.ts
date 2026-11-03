// The true cause of an empty list (lib/attribution; decision 0095, the owner 2026-11-02): the owner's cases.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { answers, pointAt } from '../src/lib/attribution.ts';

// Causes: 0 Favourites only, 1 Year, 2 Search.
const FAV = 0;
const YEAR = 1;
const SEARCH = 2;
const keys = (n: number, p: string) => Array.from({ length: n }, (_, i) => `${p}${i}`);

test('the filters leave a beautiful list, the search narrows it to nothing: the search', () => {
  // 140 pass the filter but not the words; the rest the filter keeps out, the words too.
  const found = answers([
    { causes: [SEARCH], keys: keys(140, 's') },
    { causes: [FAV, SEARCH], keys: keys(500, 'f') },
  ]);
  assert.deepEqual(pointAt(found, null), { causes: [SEARCH], frees: 140 });
});

test('the search has a list, the filter narrows it down: the filter', () => {
  const found = answers([
    { causes: [FAV], keys: keys(3, 'w') },
    { causes: [FAV, SEARCH], keys: keys(600, 'x') },
  ]);
  assert.deepEqual(pointAt(found, null), { causes: [FAV], frees: 3 });
});

test('only together: the combination, named exactly, with how many it brings back', () => {
  const found = answers([
    { causes: [FAV, YEAR], keys: keys(40, 'b') },
    { causes: [FAV, YEAR, SEARCH], keys: keys(10, 'c') },
  ]);
  assert.deepEqual(found, [{ causes: [FAV, YEAR], frees: 40 }]);
});

test('both bring something back on their own: the last change decides, the other still an answer', () => {
  const found = answers([
    { causes: [SEARCH], keys: keys(140, 's') },
    { causes: [FAV], keys: keys(3, 'w') },
  ]);
  assert.equal(found.length, 2);
  assert.deepEqual(pointAt(found, SEARCH)?.causes, [SEARCH], 'the words typed last');
  assert.deepEqual(pointAt(found, FAV)?.causes, [FAV], 'the heart tapped last');
  assert.deepEqual(pointAt(found, null)?.causes, [SEARCH], 'no change known: the one bringing most back');
});

test('a cause that brings back nothing by itself is no answer of its own', () => {
  const found = answers([{ causes: [FAV, YEAR], keys: keys(5, 'a') }]);
  assert.ok(!found.some((a) => a.causes.length === 1));
});

test('nothing kept out by anything: no answer (truly empty)', () => {
  assert.equal(pointAt(answers([]), null), null);
});
