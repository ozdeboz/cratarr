// In the app the app keeps the library (D15), and the page fetches none of it (lib/deviceSync, the real one, on a stand-in for the
// app's end of the bridge).
import { mock, test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
g.CratarrApp = { postMessage() {}, onmessage: null };
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });
localStorage.setItem('cratarr-who', '"ann"');

const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  asked.push(String(input));
  return new Response('{}', { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 2_000_000_000 });
const { FIRST_LOOK_MS, syncNow } = await import('../../src/lib/deviceSync.svelte.ts');

test('the page in the app fetches none of the library, nor the module to read it, at a start or when told', async () => {
  mock.timers.tick(FIRST_LOOK_MS * 2);
  await syncNow();
  assert.deepEqual(asked.filter((a) => a.includes('/api/index') || a.includes('index.wasm')), []);
});
