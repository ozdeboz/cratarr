// An /api/me that answers before the shelf has read itself in (lib/shelfStore, the real one, in the app's page): it has heard
// nothing of the change, listening only once it has read, and without a second look it stays on nobody's for the whole visit.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import '../harness/pageGlobals.ts';
import '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
g.CratarrApp = { postMessage() {}, onmessage: null };
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });
globalThis.fetch = (async () => new Response('{}')) as typeof fetch;

const { accountLearnt } = await import('../../src/lib/account.ts');
const { shelf } = await import('../../src/lib/shelfStore.svelte.ts');
// In the same turn as the shelf's first read: it is on its way, and not listening yet.
accountLearnt('ann');

test('the shelf is the account’s all the same', async () => {
  await wait(60);
  assert.equal(shelf.who, '@ann');
});
