// In the app the app keeps the library (D15): the page fetches none of it, says what the app tells, and passes on what is asked
// (lib/deviceSync, the real one, on a stand-in for the app's end of the bridge).
import { beforeEach, mock, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
type Port = { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null };
const sent: Record<string, unknown>[] = [];
const port: Port = { postMessage: (m) => void sent.push(JSON.parse(m)), onmessage: null };
g.CratarrApp = port;
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
const { answerMobile, FIRST_LOOK_MS, indexStands, indexSync, syncNow, tryIndexAgain } = await import('../../src/lib/deviceSync.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { indexWord } = await import('../../src/lib/deviceIndexRule.ts');

const told = (at: number | null, parts: object[], metered = false) => port.onmessage!({ data: JSON.stringify({ type: 'device-index', at, rev: at && 5, covers: at && 5, metered, parts }) });
const said = () => sent.map((m) => m.type).filter((t) => String(t).startsWith('index-'));
const part = { whole: false, stopped: false, paused: false, asks: false };
const word = () => indexWord(indexSync.shows, indexSync.at, () => 'the date');

beforeEach(() => {
  sent.length = 0;
  prefs.indexMobile = 'unasked';
});

test('the page in the app fetches none of the library, nor the module to read it, at a start or when told', async () => {
  mock.timers.tick(FIRST_LOOK_MS * 2);
  await syncNow();
  assert.deepEqual(asked.filter((a) => a.includes('/api/index') || a.includes('index.wasm')), []);
});

test('a look asked for is the app\'s', async () => {
  await syncNow();
  assert.deepEqual(said(), ['index-look']);
  assert.equal(sent.at(-1)?.told, false);
});

test('what /api/changes says of the library goes to the app, once it holds some, and whether it told', async () => {
  told(null, []);
  await indexStands({ rev: 5, changed: true });
  assert.deepEqual(said(), [], 'nothing held: nothing to bring up to date');
  told(1, [part]);
  await indexStands({ rev: 5, changed: false });
  assert.deepEqual(said(), []);
  await indexStands({ rev: 5, changed: true });
  assert.deepEqual(said(), ['index-look']);
  assert.equal(sent.at(-1)?.told, true);
  await indexStands({ rev: 6, changed: false });
  assert.deepEqual(said(), ['index-look', 'index-look'], 'a cover');
  assert.equal(sent.at(-1)?.told, false);
});

test('This device says what the app tells, with its date and where it stands', () => {
  told(1_000, [part, part]);
  assert.equal(indexSync.at, 1_000);
  assert.equal(indexSync.rev, 5);
  assert.equal(indexSync.covers, 5);
  assert.equal(word(), 'the date');
  told(null, []);
  assert.equal(indexSync.at, null);
  assert.equal(word(), 'Not yet');
  told(1_000, [{ ...part, stopped: true }, part]);
  assert.equal(word(), 'Differs from Cratarr');
  told(1_000, [{ ...part, paused: true }]);
  assert.equal(word(), 'Paused');
});

test('the app\'s question is asked here, and a yes goes back as a look', async () => {
  told(null, [{ ...part, whole: true, asks: true }, part], true);
  assert.equal(indexSync.shows, 'asks');
  await answerMobile(true);
  assert.equal(prefs.indexMobile, 'yes');
  assert.deepEqual(said(), ['index-look']);
});

test('a no waits for Wi-Fi, and the app\'s word from before the answer does not ask again', async () => {
  told(null, [{ ...part, whole: true, asks: true }], true);
  await answerMobile(false);
  assert.equal(indexSync.shows, 'wifi');
  told(null, [{ ...part, whole: true, asks: true }], true);
  assert.equal(indexSync.shows, 'wifi');
});

test('a tap asks the app to try again, and a no is asked again', async () => {
  prefs.indexMobile = 'no';
  await tryIndexAgain();
  assert.equal(prefs.indexMobile, 'unasked');
  assert.deepEqual(said(), ['index-again']);
});

test('This device has the row in the app too', () => {
  const part = readFileSync(new URL('../../src/lib/settings/DevicePart.svelte', import.meta.url), 'utf8');
  const row = part.indexOf('name="Library on this device"');
  assert.ok(row > 0);
  assert.doesNotMatch(part.slice(part.lastIndexOf('<Group', row), row), /inApp/);
});
