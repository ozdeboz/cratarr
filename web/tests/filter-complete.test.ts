// A filter row asks nothing until it says what (lib/filterRow `complete`; the owner, 2026-10-29: a new Type row with
// no value chosen emptied the Library, "0 of 2,972"). The cases are the server's too: crates.rs Condition::complete()
// reads the same fixture, so the page and the server skip the same rows.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { complete } = await import('../src/lib/filterRow.ts');
type Case = { condition: Parameters<typeof complete>[0]; complete: boolean };
const { complete: cases } = JSON.parse(readFileSync(new URL('./fixtures/conditions.json', import.meta.url), 'utf8')) as { complete: Case[] };

test('every row of the shared fixture asks what the server says it asks', () => {
  assert.ok(cases.length >= 10);
  for (const c of cases) assert.equal(complete(c.condition), c.complete, JSON.stringify(c.condition));
});

test('a changed row is a new row; on and Any are said by leaving them out', async () => {
  const { changed } = await import('../src/lib/filterRow.ts');
  const row = { axis: 'type', not: false, value: { kind: 'any_of', values: ['Album'] }, enabled: false, quantifier: 'all' } as Parameters<typeof complete>[0];
  const notted = changed(row, { not: true });
  assert.notEqual(notted, row);
  assert.equal(row.not, false, 'the old row stays as it was');
  assert.deepEqual(notted, { axis: 'type', not: true, value: { kind: 'any_of', values: ['Album'] }, enabled: false, quantifier: 'all' });
  assert.deepEqual(changed(row, { enabled: undefined, quantifier: undefined }), { axis: 'type', not: false, value: { kind: 'any_of', values: ['Album'] } });
  assert.deepEqual(changed(row, { quantifier: 'any' }), { axis: 'type', not: false, value: { kind: 'any_of', values: ['Album'] }, enabled: false });
});
