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
