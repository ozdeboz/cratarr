// The library on this device's own database (lib/indexStore; the review of 2026-11-07, its sixth finding): an open that failed is tried
// again, not remembered for the page's life, and a connection another tab's upgrade waits on goes, the next read opening anew.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openConnections, opening, upgradeElsewhere } from '../harness/fakeIdb.ts';

const { readPart, writePart } = await import('../../src/lib/indexStore.ts');
const kept = { rev: 5, at: 1, wants: 1, part: { whole: false, stopped: false, lost: 0, failed: 0, tried: null } };

test('an open that failed is tried again at the next read or write, not remembered for the page’s life', async () => {
  opening.refused = 1;
  assert.equal(await writePart('@ann', 'index', kept), false, 'refused this once');
  assert.equal(await writePart('@ann', 'index', kept), true);
  assert.deepEqual(await readPart('@ann', 'index'), kept);
});

test('another tab’s upgrade closes the connection here, and the next read opens anew', async () => {
  await writePart('@bo', 'index', kept);
  assert.equal(openConnections('cratarr-index@bo'), 1);
  upgradeElsewhere('cratarr-index@bo');
  assert.equal(openConnections('cratarr-index@bo'), 0, 'nothing here holds the other tab back');
  assert.deepEqual(await readPart('@bo', 'index'), kept);
  assert.equal(openConnections('cratarr-index@bo'), 1);
});
