// A browser saying it has no network (lib/reach on lib/deviceIndex, the real ones): nothing to settle, as there is no blip to wait out.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const heard: Record<string, () => void> = {};
const g = globalThis as Record<string, unknown>;
g.window = globalThis;
g.addEventListener = (name: string, f: () => void) => void (heard[name] = f);
Object.defineProperty(globalThis, 'navigator', { value: { onLine: false }, configurable: true });

const { reach } = await import('../../src/lib/reach.svelte.ts');
const { readsNow } = await import('../../src/lib/deviceIndex.svelte.ts');

test('a page opening with no network reads the device at once', () => {
  assert.equal(reach.away, true);
  assert.deepEqual(readsNow(), { from: 'device', why: 'no-network' });
});

test('the network back, reads are the server\'s again, with nothing settled against it', () => {
  heard.online();
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
});

test('the network going is at once too', () => {
  heard.offline();
  assert.equal(reach.away, true);
  assert.deepEqual(readsNow(), { from: 'device', why: 'no-network' });
});
