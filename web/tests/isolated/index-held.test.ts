// What one index.wasm instance holds goes together (audit/OFFLINE-REVIEW.md, finding 5): a device opened over an index answers from
// that index alone, and an account's own is the one loaded for it. Another index held, by a load or by a feed applied, lets the device
// over the one before go, and the account's own with it, so nothing answers from what is no longer held, and no second copy of the
// index stays in memory behind a device.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { instantiateIndex } = await import('../../src/lib/indexEngine.ts');

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const module = await WebAssembly.compile(readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url)));
const engine = () => instantiateIndex(module);

/** Any ask a device answers over an index with nothing else held. */
const ROWS = { ask: 'rows', id: 'genre:none', offset: 0, limit: 1, now: 1 };

test('another index loaded lets the device over the one before go', async () => {
  const e = await engine();
  assert.equal(e.loadIndex(vector('whole.bin')), null);
  assert.ok(e.openDevice());
  assert.notEqual(e.ask(ROWS), null, 'a device answers over what it was opened on');
  assert.equal(e.loadIndex(vector('after.bin')), null);
  assert.equal(e.ask(ROWS), null, 'nothing answers from the index before');
  assert.ok(e.openDevice());
  assert.notEqual(e.ask(ROWS), null, 'opened again, over the one held now');
});

test('a feed applied lets the device go too, and what is held is the index after it', async () => {
  const e = await engine();
  e.loadIndex(vector('whole.bin'));
  assert.ok(e.openDevice());
  assert.equal(e.applyIndex(vector('changes.bin')), null);
  assert.equal(e.ask(ROWS), null);
  assert.deepEqual(e.saveIndex(), vector('after.bin'));
});

test("an account's own goes with the index it was loaded beside", async () => {
  const e = await engine();
  e.loadIndex(vector('whole.bin'));
  assert.equal(e.loadOwn(vector('own.bin')), null);
  assert.ok(e.ownAt() >= 0);
  e.loadIndex(vector('after.bin'));
  assert.equal(e.ownAt(), -1, 'none held for the new one until it is loaded');
});
