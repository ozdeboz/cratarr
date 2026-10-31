// A record's tracks kept apart in a play order (lib/spread; the owner, 2026-10-31: a grabbed crate's Play "had too much
// repetition of records in the queue").
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { spreadRecords } = await import('../src/lib/spread.ts');

const rec = (s: string) => s[0];
/** How few other tracks came between two of one record, at least, over the whole order. */
function closest(order: string[]): number {
  let least = Infinity;
  const last = new Map<string, number>();
  order.forEach((t, i) => {
    const at = last.get(rec(t));
    if (at !== undefined) least = Math.min(least, i - at - 1);
    last.set(rec(t), i);
  });
  return least;
}

test('a crate in closeness order: no record twice within four others, the order kept as far as it can be', () => {
  // Closeness order with record A crowding the top, as a "like" crate's does.
  const order = ['A1', 'A2', 'A3', 'B1', 'C1', 'A4', 'D1', 'E1', 'B2', 'F1', 'C2', 'G1', 'D2', 'H1', 'E2', 'I1'];
  const out = spreadRecords(order, rec);
  assert.equal(out.length, order.length);
  assert.deepEqual([...out].sort(), [...order].sort());
  assert.ok(closest(out) >= 4, out.join(' '));
  // The closest stays first; a record's own tracks stay in their order.
  assert.equal(out[0], 'A1');
  assert.deepEqual(out.filter((t) => t[0] === 'A'), ['A1', 'A2', 'A3', 'A4']);
});

test('fewer records than the gap: as far apart as they can be, never back to back where avoidable', () => {
  const order = ['A1', 'A2', 'A3', 'B1', 'B2', 'C1', 'C2'];
  const out = spreadRecords(order, rec);
  assert.ok(closest(out.slice(0, 6)) >= 2, out.join(' '));
  assert.deepEqual(out.slice(0, 3).map(rec).sort(), ['A', 'B', 'C']);
});

test('one record, or too few tracks: the order as given', () => {
  assert.deepEqual(spreadRecords(['A1', 'A2', 'A3'], rec), ['A1', 'A2', 'A3']);
  assert.deepEqual(spreadRecords(['A1', 'B1'], rec), ['A1', 'B1']);
  assert.deepEqual(spreadRecords([], rec), []);
});

test("when one record's tracks are all that is left, they come in their order", () => {
  const out = spreadRecords(['A1', 'A2', 'A3', 'A4', 'B1'], rec);
  assert.deepEqual(out, ['A1', 'B1', 'A2', 'A3', 'A4']);
});

test('distinct records keep the order exactly', () => {
  const order = ['A1', 'B1', 'C1', 'D1', 'E1', 'F1'];
  assert.deepEqual(spreadRecords(order, rec), order);
});
