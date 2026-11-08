// What a device answers for itself, in the app: the app holds the one copy, so the page opens none and asks at its address (lib/deviceAnswers,
// the real one, on a stand-in for the app's end).
import { beforeEach, test } from 'node:test';
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

let answer: (() => Response) | null = null;
const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  asked.push(String(input));
  return answer ? answer() : new Response('', { status: 404 });
}) as typeof fetch;

const { deviceAsk, deviceCrateRows } = await import('../../src/lib/deviceAnswers.svelte.ts');

const said = (ask: string) => JSON.parse(decodeURIComponent(ask.slice(ask.indexOf('ask=') + 4)));

beforeEach(() => {
  answer = null;
  asked.length = 0;
});

test('what is asked goes to the app whole, and its answer comes back', async () => {
  answer = () => new Response('[{"id":"hearted"}]', { headers: { 'content-type': 'application/json' } });
  assert.deepEqual(await deviceAsk({ ask: 'crates', now: 1 }), [{ id: 'hearted' }]);
  assert.equal(asked.length, 1);
  assert.ok(asked[0].startsWith('/__device/answer?ask='));
  assert.deepEqual(said(asked[0]), { ask: 'crates', now: 1 });
});

test('with nothing to say, it is no answer', async () => {
  assert.equal(await deviceAsk({ ask: 'crates', now: 1 }), null);
});

test('a crate\'s rows are asked as a page looks at it', async () => {
  answer = () => new Response('{"total":0}', { headers: { 'content-type': 'application/json' } });
  await deviceCrateRows('dub', 5, 10, undefined, 'only', undefined, { order: 'title', down: true, without: ['cratarr//x'], filter: [] });
  const look = said(asked[0]);
  assert.deepEqual({ ...look, now: 0 }, { ask: 'rows', id: 'dub', offset: 5, limit: 10, favourites: 'only', without: ['cratarr//x'], filter: [], order: 'title', down: true, now: 0 });
});

test('another library than Cratarr\'s own is nothing a device holds', async () => {
  assert.equal(await deviceCrateRows('dub', 0, 10, 'jellyfin', undefined, undefined, {}), null);
  assert.deepEqual(asked, []);
});
