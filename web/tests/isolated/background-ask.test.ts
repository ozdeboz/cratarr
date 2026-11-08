// What playing in the background needs, asked once as the app starts (lib/askAtStart, the real one, on a stand-in for the app's
// end of the bridge): a shortcut to This device's Background row. App's own mounting of it is read from its source, which says
// the line is there, not that it works.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { effects } from '../harness/pageGlobals.ts';

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
globalThis.fetch = (async () => new Response('{}', { headers: { 'content-type': 'application/json' } })) as typeof fetch;

const { appInfo } = await import('../../src/lib/app.svelte.ts');
const { backgroundAsk } = await import('../../src/lib/askAtStart.svelte.ts');
const { DEFAULTS, fromSaved, prefs } = await import('../../src/lib/prefs.svelte.ts');
const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });

beforeEach(() => {
  prefs.backgroundAsked = false;
  appInfo.version = null;
  appInfo.batteryAllowed = true;
  toTheApp.length = 0;
});

test('not asked yet on a new device', () => assert.equal(DEFAULTS.backgroundAsked, false));

test('nothing is asked before the app has said how it stands', () => {
  appInfo.batteryAllowed = false;
  assert.equal(backgroundAsk.due, false);
});

test('Android may stop the app: it asks', () => {
  fromApp({ type: 'app', version: '0.16.0', batteryAllowed: false });
  assert.equal(backgroundAsk.due, true);
});

test('allowed already: nothing to ask', () => {
  fromApp({ type: 'app', version: '0.16.0', batteryAllowed: true });
  assert.equal(backgroundAsk.due, false);
});

test("yes is Android's own dialog, as the Background row opens it", () => {
  backgroundAsk.allow();
  assert.deepEqual(toTheApp, [{ type: 'battery' }]);
});

test('answered or refused it is not asked again, Android saying what it may', () => {
  fromApp({ type: 'app', version: '0.16.0', batteryAllowed: false });
  backgroundAsk.asked();
  assert.equal(backgroundAsk.due, false);
  fromApp({ type: 'app', version: '0.16.0', batteryAllowed: false });
  assert.equal(backgroundAsk.due, false, 'at the next start too');
});

test('the answer is the device\'s and outlasts the page', () => {
  backgroundAsk.asked();
  for (const run of effects) run();
  assert.equal(JSON.parse(localStorage.getItem('cratarr-prefs')!).backgroundAsked, true);
  assert.equal(fromSaved('backgroundAsked', true), true, 'and it is read back');
});

test('the shell shows it, whatever the page', () => {
  const app = readFileSync(new URL('../../src/App.svelte', import.meta.url), 'utf8');
  assert.ok(app.includes('<BackgroundAsk />'));
});
