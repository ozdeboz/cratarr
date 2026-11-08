// The device's own store of the library (lib/indexStore) opening its database (audit/OFFLINE-REVIEW.md, finding 6): an open that failed
// or was blocked is asked again at the next read or write, not remembered for the page's life; a connection another tab's upgrade
// needs is let go; and one that opens after its page stopped waiting is closed, never left to block the next.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { connections, opening, upgradeElsewhere } from '../harness/fakeIdb.ts';

const { readPart, writePart } = await import('../../src/lib/indexStore.ts');

const KEPT = { rev: 5, at: 1, wants: 1, part: { whole: false, stopped: false, lost: 0, failed: 0, tried: null } };
let n = 0;
const someone = () => `@store${++n}`;
const turns = () => new Promise((done) => setTimeout(done, 5));

test('an open that failed is asked again at the next write', async () => {
  const who = someone();
  opening.fails = 1;
  assert.equal(await writePart(who, 'index', KEPT), false, 'nothing kept while it cannot open');
  assert.equal(await writePart(who, 'index', KEPT), true, 'asked again, it opens');
  assert.deepEqual(await readPart(who, 'index'), KEPT);
});

test('an open blocked by another tab is asked again, and the connection that came late is closed', async () => {
  const who = someone();
  opening.blocked = 1;
  assert.equal(await writePart(who, 'index', KEPT), false);
  await turns();
  const late = connections.filter((c) => c.name === `cratarr-index${who}`);
  assert.ok(late.length >= 1 && late.every((c) => c.closed), 'nobody holds what opened after it was given up');
  assert.equal(await writePart(who, 'index', KEPT), true);
});

test("another tab's upgrade: this one lets go of its connection, and opens anew when next asked", async () => {
  const who = someone();
  assert.equal(await writePart(who, 'index', KEPT), true);
  const name = `cratarr-index${who}`;
  upgradeElsewhere(name);
  assert.ok(connections.filter((c) => c.name === name).every((c) => c.closed), 'let go, so the upgrade is not blocked');
  assert.equal(await writePart(who, 'index', KEPT), true, 'and opened again');
  assert.equal(connections.filter((c) => c.name === name && !c.closed).length, 1);
});
