// The moment's weather with Cratarr out of reach, in a browser (decision 0101, point 18; lib/momentHere, the real one): where the
// device roughly is goes to Open-Meteo only with the switch on, Cratarr away and the internet there, at most every half hour.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

/** Every request made, and how many times the device was asked where it is. */
let asked: string[] = [];
let placesAsked = 0;
/** What the device says of where it is, and what Open-Meteo answers; null: it fails. */
let place: { latitude: number; longitude: number } | null = { latitude: 52.08371, longitude: 5.11782 };
let answer: unknown = { current: { weather_code: 61, cloud_cover: 90, temperature_2m: 9, is_day: 0 } };

Object.defineProperty(globalThis, 'navigator', {
  value: {
    onLine: true,
    geolocation: {
      getCurrentPosition(ok: (p: { coords: typeof place }) => void, no: () => void) {
        placesAsked++;
        if (place) ok({ coords: place });
        else no();
      },
    },
  },
  configurable: true,
});
globalThis.fetch = (async (input: string) => {
  asked.push(String(input));
  if (answer === null) throw new TypeError('Failed to fetch');
  return new Response(JSON.stringify(answer), { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

const { momentHere, momentNow, weatherHere } = await import('../../src/lib/momentHere.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { networkSaid, reach } = await import('../../src/lib/reach.svelte.ts');
const { network } = await import('../../src/lib/away.svelte.ts');

const HALF_HOUR = 30 * 60_000;
const nav = navigator as unknown as { onLine: boolean };
/** 21:30 on this device's clock. */
const evening = (ms = 0) => new Date(new Date(2026, 10, 7, 21, 30).getTime() + ms);

beforeEach(() => {
  localStorage.removeItem('cratarr-moment-weather');
  asked = [];
  placesAsked = 0;
  place = { latitude: 52.08371, longitude: 5.11782 };
  answer = { current: { weather_code: 61, cloud_cover: 90, temperature_2m: 9, is_day: 0 } };
  prefs.momentWeather = true;
  reach.away = true;
  nav.onLine = true;
  network.validated = null;
});

test('off by default, and kept once turned on', async () => {
  const { DEFAULTS, fromSaved } = await import('../../src/lib/prefs.svelte.ts');
  assert.equal(DEFAULTS.momentWeather, false);
  assert.equal(fromSaved('momentWeather', true), true);
});

test('with the switch off, no place is looked for and nothing is asked', async () => {
  prefs.momentWeather = false;
  assert.equal(await weatherHere(), null);
  assert.equal(placesAsked, 0);
  assert.deepEqual(asked, []);
});

test('with Cratarr there, nothing is asked: it has the weather', async () => {
  reach.away = false;
  assert.equal(await weatherHere(), null);
  assert.equal(placesAsked, 0);
  assert.deepEqual(asked, []);
});

test('the switch on, Cratarr away, the internet there: Open-Meteo alone is asked, for a coarse place', async () => {
  const m = await momentHere(['al-1'], evening());
  assert.equal(asked.length, 1);
  const url = new URL(asked[0]);
  assert.equal(url.host, 'api.open-meteo.com');
  assert.equal(url.searchParams.get('latitude'), '52.1');
  assert.equal(url.searchParams.get('longitude'), '5.1');
  assert.ok(!asked[0].includes('083') && !asked[0].includes('117'), 'none of the place beyond its first decimal');
  assert.deepEqual(m, { name: 'Music for a Rainy Evening', sky: 'rainy', dark: true, albums: ['al-1'], timeOnly: false });
});

test('the place itself is kept nowhere', async () => {
  await weatherHere(evening().getTime());
  const kept = localStorage.getItem('cratarr-moment-weather') ?? '';
  assert.ok(kept.includes('rainy'), 'the weather is');
  assert.ok(!kept.includes('52') && !kept.includes('5.1'), kept);
});

test('asked at most every half hour: the answer stands until then, and only then is the next one asked', async () => {
  const t = evening().getTime();
  await weatherHere(t);
  assert.equal((await weatherHere(t + HALF_HOUR - 1))?.condition, 'rainy');
  assert.equal(asked.length, 1);
  assert.equal(placesAsked, 1);
  answer = { current: { weather_code: 0, cloud_cover: 0, temperature_2m: 9, is_day: 0 } };
  assert.equal((await weatherHere(t + HALF_HOUR))?.condition, 'clear-night');
  assert.equal(asked.length, 2);
});

test('an ask that failed counts: no second one for half an hour', async () => {
  const t = evening().getTime();
  answer = null;
  assert.equal(await weatherHere(t), null);
  answer = { current: { weather_code: 0, is_day: 1 } };
  assert.equal(await weatherHere(t + 60_000), null);
  assert.equal(asked.length, 1);
  assert.equal((await weatherHere(t + HALF_HOUR))?.condition, 'sunny');
});

test('two reads at once are one ask', async () => {
  const t = evening().getTime();
  await Promise.all([weatherHere(t), weatherHere(t), weatherHere(t)]);
  assert.equal(asked.length, 1);
});

test('no internet: nothing is asked, it goes by time and day only, and says so', async () => {
  nav.onLine = false;
  const m = await momentHere([], evening());
  assert.deepEqual(asked, []);
  assert.equal(placesAsked, 0);
  assert.deepEqual(m, { name: 'Music for the Evening', sky: 'plain', dark: true, albums: [], timeOnly: true });
});

test("Android's word that the network has no internet counts as none", async () => {
  network.validated = false;
  assert.equal((await momentHere([], evening())).timeOnly, true);
  assert.deepEqual(asked, []);
});

test('the internet back: it is asked at once, as nothing was asked before', async () => {
  nav.onLine = false;
  await weatherHere(evening().getTime());
  nav.onLine = true;
  assert.equal((await weatherHere(evening(60_000).getTime()))?.condition, 'rainy');
});

test('a device that cannot say where it is: time and day only, nothing sent, and it tries again at the next read', async () => {
  place = null;
  const m = await momentHere([], evening());
  assert.equal(m.timeOnly, true);
  assert.deepEqual(asked, []);
  place = { latitude: 1, longitude: 2 };
  assert.equal((await momentHere([], evening(60_000))).timeOnly, false);
});

test('switched off while the device looks for where it is: nothing leaves it', async () => {
  const geo = navigator.geolocation as unknown as { getCurrentPosition: (ok: (p: unknown) => void) => void };
  const look = geo.getCurrentPosition;
  let found: (p: unknown) => void = () => {};
  geo.getCurrentPosition = (ok) => void (found = ok);
  const read = weatherHere(evening().getTime());
  prefs.momentWeather = false;
  found({ coords: place });
  assert.equal(await read, null);
  assert.deepEqual(asked, []);
  geo.getCurrentPosition = look;
});

test('Cratarr back while the device looks: nothing leaves it either', async () => {
  const geo = navigator.geolocation as unknown as { getCurrentPosition: (ok: (p: unknown) => void) => void };
  const look = geo.getCurrentPosition;
  let found: (p: unknown) => void = () => {};
  geo.getCurrentPosition = (ok) => void (found = ok);
  const read = weatherHere(evening().getTime());
  reach.away = false;
  found({ coords: place });
  assert.equal(await read, null);
  assert.deepEqual(asked, []);
  geo.getCurrentPosition = look;
});

test('an answer that is no weather is time and day only', async () => {
  answer = { error: true, reason: 'no' };
  assert.equal((await momentHere([], evening())).timeOnly, true);
});

test('weather older than the half hour is not used for want of a new one', async () => {
  const t = evening().getTime();
  await weatherHere(t);
  nav.onLine = false;
  assert.equal(await weatherHere(t + HALF_HOUR), null);
});

test('with reads on the device it is worked out here, from the albums Cratarr last gave, and Cratarr is asked nothing', async () => {
  networkSaid(false);
  place = null;
  const m = await momentNow(undefined, [], ['al-1', 'al-2']);
  networkSaid(true);
  assert.deepEqual(m.albums, ['al-1', 'al-2']);
  assert.equal(m.timeOnly, true);
  assert.deepEqual(asked, []);
});

test('with none held, its last copy\'s albums, under a name worked out here and never the copy\'s', async () => {
  networkSaid(false);
  place = null;
  answer = { name: 'Music from Before', sky: 'sunny', dark: false, albums: ['al-9'] };
  const m = await momentNow(undefined, [], undefined);
  networkSaid(true);
  assert.deepEqual(m.albums, ['al-9']);
  assert.notEqual(m.name, 'Music from Before');
});

test('no albums to go on: the read goes where any other goes', async () => {
  networkSaid(false);
  answer = { name: 'Music from Before', sky: 'sunny', dark: false, albums: [] };
  const m = await momentNow(undefined, [], undefined);
  networkSaid(true);
  assert.equal(m.name, 'Music from Before');
});

test('with Cratarr answering it is its own moment, whatever is held', async () => {
  answer = { name: 'Its Own', sky: 'sunny', dark: false, albums: ['al-3'] };
  assert.equal((await momentNow(undefined, [], ['al-1'])).name, 'Its Own');
});

test('it goes by this device\'s clock', async () => {
  place = null;
  assert.equal((await momentHere([], new Date(2026, 10, 7, 9, 0))).name, 'Music for the Morning');
  assert.equal((await momentHere([], new Date(2026, 10, 7, 9, 0))).dark, false);
});
