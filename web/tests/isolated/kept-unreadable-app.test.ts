// The app's kept list that cannot be read (lib/kept, the real one, on a stand-in for the app's end of the bridge): the page hears it,
// and asks the app to start it again.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
const toTheApp: Record<string, unknown>[] = [];
const bridge: { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null } = { postMessage: (m) => void toTheApp.push(JSON.parse(m)), onmessage: null };
g.CratarrApp = bridge;
g.window = globalThis;

const { kept, setKept, startKeptAgain } = await import('../../src/lib/kept.svelte.ts');
const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });

test('the app saying its list cannot be read is heard', () => {
  fromApp({ type: 'kept', items: [], used: 0, budget: null, accounts: [], waiting: null, current: null, running: false, unreadable: true });
  assert.equal(kept.unreadable, true);
});

test('nothing is synced while the list cannot be read: the page shows none, and the app is not asked', () => {
  const asked = toTheApp.length;
  setKept('album', 'cratarr', 'al-1', true, 'A record');
  assert.deepEqual(kept.items, []);
  assert.equal(toTheApp.length, asked);
});

test('starting again is the app’s to do', () => {
  startKeptAgain();
  assert.deepEqual(toTheApp.at(-1), { type: 'kept-start-again' });
});

test('a report that says nothing of it is a list that reads', () => {
  fromApp({ type: 'kept', items: [], used: 0, budget: null, accounts: [], waiting: null, current: null, running: false });
  assert.equal(kept.unreadable, false);
});
