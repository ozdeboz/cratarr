// The app saying the device has no network (lib/away's `network`, Reach.kt's word): an input to where reads come from, and nothing
// the app decides. The real modules, on a stand-in for the app's end of the bridge.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
const bridge: { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null } = { postMessage() {}, onmessage: null };
g.CratarrApp = bridge;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });
globalThis.fetch = (async () => new Response('{}', { headers: { 'content-type': 'application/json' } })) as typeof fetch;

await import('../../src/lib/away.svelte.ts');
const { readsNow } = await import('../../src/lib/deviceIndex.svelte.ts');
const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });

test('the app saying there is no network moves the reads at once', () => {
  fromApp({ type: 'network', online: false, validated: false, kind: 'none' });
  assert.deepEqual(readsNow(), { from: 'device', why: 'no-network' });
});

test('its saying there is one again moves them back', () => {
  fromApp({ type: 'network', online: true, validated: true, kind: 'wifi' });
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
});

// CU13 (red first: the app's word carried no Mobile data): on mobile data with Sync's Mobile data set to None, the app says so in its
// network word, and reads come from the device at once, with nothing to settle; off mobile data, or set otherwise, back to Cratarr.
test("the app saying Mobile data is None while on it moves the reads to the device, saying why", () => {
  fromApp({ type: 'network', online: true, validated: true, kind: 'cellular', mobileOff: true });
  assert.deepEqual(readsNow(), { from: 'device', why: 'mobile' });
  fromApp({ type: 'network', online: true, validated: true, kind: 'wifi', mobileOff: false });
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
});

test('no network says so over Mobile data', () => {
  fromApp({ type: 'network', online: false, validated: false, kind: 'none', mobileOff: true });
  assert.deepEqual(readsNow(), { from: 'device', why: 'no-network' });
  fromApp({ type: 'network', online: true, validated: true, kind: 'wifi', mobileOff: false });
});
