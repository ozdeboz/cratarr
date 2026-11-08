// A session ended in Cratarr's mode (lib/app, lib/reach, the real ones; decision 0101): the sign-in page says nobody itself, as a
// session of the other address's may yet be taken for it, so the page tells the app nothing. A process of its own: a page goes
// to sign in once, and lib/reach remembers, so in a file with any other sign-in this would find it gone already.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
const toTheApp: Record<string, unknown>[] = [];
const bridge: { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null } = { postMessage: (m) => void toTheApp.push(JSON.parse(m)), onmessage: null };
g.CratarrApp = bridge;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });
let goneTo: string | null = null;
Object.assign(g.location as object, { assign: (to: string) => void (goneTo = to) });
globalThis.fetch = (async () => new Response('{}')) as typeof fetch;

await import('../../src/lib/app.svelte.ts');
const { toSignIn } = await import('../../src/lib/reach.svelte.ts');

test('in Cratarr’s mode the sign-in page says it, and the page goes there', () => {
  bridge.onmessage!({ data: JSON.stringify({ type: 'app', version: '1', mode: 'cratarr' }) });
  toSignIn();
  assert.deepEqual(toTheApp.filter((m) => m.type === 'nobody'), []);
  assert.match(String(goneTo), /^\/signin/);
});
