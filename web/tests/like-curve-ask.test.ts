// Sounds like's curve as asked (lib/likeCurveAsk, 0.11): within a crate's other conditions by POST, its Sounds like and
// those switched off passed over; without any, the GET as before; asked again only when what it counts changes.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { withinOf, curveQuery, curveBody, asksWithin, curveKey } = await import('../src/lib/likeCurveAsk.ts');

const genre = { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Jazz'] } };
const years = { axis: 'year', not: false, value: { kind: 'range', min: 1970, max: 1979 } };
const like = (min?: number) => ({ axis: 'like', not: false, value: { kind: 'like', of: ['ar-1'], count: 1000, ...(min !== undefined ? { min } : {}) } });
const off = { ...years, enabled: false };

test('within: a Sounds like and a condition switched off are passed over', () => {
  assert.deepEqual(withinOf([genre, like(60), off, years]), [genre, years]);
  assert.deepEqual(withinOf([]), []);
});

test('the POST only with conditions to count within', () => {
  assert.equal(asksWithin({ of: ['ar-1'] }), false);
  assert.equal(asksWithin({ of: ['ar-1'], within: [like(60)] }), false);
  assert.equal(asksWithin({ of: ['ar-1'], within: [off] }), false);
  assert.equal(asksWithin({ of: ['ar-1'], within: [genre, like(60)] }), true);
});

test('the GET query as before 0.11', () => {
  const q = curveQuery({ of: ['ar-1', 'al-2'], without: ['cratarr/incoming'], fresh: true, own: true, weights: [1, 0.33333] }, 'cratarr');
  assert.equal(q.get('of'), 'ar-1,al-2');
  assert.equal(q.get('source'), 'cratarr');
  assert.equal(q.get('without'), 'cratarr/incoming');
  assert.equal(q.get('fresh'), 'true');
  assert.equal(q.get('own'), 'true');
  assert.equal(q.get('weights'), '1,0.333');
  assert.equal(curveQuery({ of: ['ar-1'] }).toString(), 'of=ar-1');
});

test("the POST body: seeds as a list, the conditions within, only what's set", () => {
  assert.deepEqual(curveBody({ of: ['ar-1'], within: [genre, like(60)], own: false, fresh: false }), { of: ['ar-1'], conditions: [genre] });
  assert.deepEqual(curveBody({ of: ['ar-1'], within: [genre], without: ['cratarr/incoming'], own: true, weights: [0.5] }), {
    of: ['ar-1'],
    conditions: [genre],
    without: ['cratarr/incoming'],
    own: true,
    weights: [0.5],
  });
});

test("the key: the cut moving or a condition switched off doesn't ask again; a chip does", () => {
  const at = (within: unknown[], own = false) => curveKey({ of: ['ar-1'], within: within as never, own });
  assert.equal(at([genre, like(60)]), at([genre, like(45)]));
  assert.equal(at([genre]), at([genre, off]));
  assert.notEqual(at([genre]), at([genre, years]));
  assert.notEqual(at([genre]), at([genre], true));
  assert.notEqual(curveKey({ of: ['ar-1'] }, 'cratarr'), curveKey({ of: ['ar-1'] }));
});
