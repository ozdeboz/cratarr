// The moment's weather in the app (lib/momentHere, the real one, on a stand-in for the app's end of the bridge): the app says
// where the device roughly is, as a WebView gives a page on a plain address nothing. Turning the switch on asks for the permission.
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

/** What left for anywhere but Cratarr. */
const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  if (String(input).startsWith('http')) asked.push(String(input));
  return new Response(JSON.stringify({ current: { weather_code: 3, cloud_cover: 80, temperature_2m: 12, is_day: 1 } }), { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

const { askForPlace, weatherHere } = await import('../../src/lib/momentHere.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { reach } = await import('../../src/lib/reach.svelte.ts');
const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });
const told = (type: string) => toTheApp.filter((m) => m.type === type);

test('turning the switch on asks the app for the permission, and nothing else goes anywhere', () => {
  askForPlace();
  assert.deepEqual(told('location'), [{ type: 'location', op: 'ask' }]);
  assert.deepEqual(asked, []);
});

test('the app is asked where the device is, and what it says is what is asked of Open-Meteo', async () => {
  prefs.momentWeather = true;
  reach.away = true;
  const read = weatherHere(1_000_000);
  await new Promise((go) => setImmediate(go));
  assert.deepEqual(told('location').at(-1), { type: 'location' });
  fromApp({ type: 'location', lat: 48.13, lon: 11.58 });
  assert.equal((await read)?.condition, 'cloudy');
  const url = new URL(asked[0]);
  assert.equal(url.searchParams.get('latitude'), '48.1');
  assert.equal(url.searchParams.get('longitude'), '11.6');
});

test('the app having no place to give: nothing is sent', async () => {
  localStorage.removeItem('cratarr-moment-weather');
  asked.length = 0;
  const read = weatherHere(5_000_000);
  await new Promise((go) => setImmediate(go));
  fromApp({ type: 'location' });
  assert.equal(await read, null);
  assert.deepEqual(asked, []);
});
