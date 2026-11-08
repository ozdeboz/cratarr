// The moment worked out on the device (lib/moment): its name and sky as Cratarr's `app::moment` has them, on that file's own
// cases, and Open-Meteo's words read as Home Assistant's.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { askFor, coarse, conditionOf, momentOf, weatherOf } = await import('../src/lib/moment.ts');

test('without the weather it goes by the part of the day alone, on a plain sky', () => {
  assert.equal(momentOf(null, 2).name, 'Music for the Night');
  assert.equal(momentOf(null, 14).name, 'Music for the Afternoon');
  assert.equal(momentOf(null, 9).name, 'Music for the Morning');
  assert.equal(momentOf(null, 20).name, 'Music for the Evening');
  assert.equal(momentOf(null, 14).sky, 'plain');
});

test('the parts of the day turn where Cratarr turns them', () => {
  const part = (h: number) => momentOf(null, h).name.split(' ').at(-1);
  assert.deepEqual([4.9, 5, 11.9, 12, 16.9, 17, 22.9, 23].map(part), ['Night', 'Morning', 'Morning', 'Afternoon', 'Afternoon', 'Evening', 'Evening', 'Night']);
});

test('dark goes by the sun where it is told, else by the clock from seven to seven', () => {
  assert.deepEqual([6.9, 7, 18.9, 19].map((h) => momentOf(null, h).dark), [true, false, false, true]);
  assert.equal(momentOf({ condition: 'cloudy', day: true }, 22).dark, false);
  assert.equal(momentOf({ condition: 'cloudy', day: false }, 12).dark, true);
});

test("the server's cases read the same here", () => {
  const sunny = momentOf({ condition: 'sunny', cloud: 5, temperature: 18, day: true }, 15);
  const rainy = momentOf({ condition: 'pouring', cloud: 100, temperature: 11, day: true }, 15);
  assert.deepEqual([sunny.name, sunny.sky, sunny.dark, rainy.name, rainy.sky], ['Music for a Sunny Afternoon', 'sunny', false, 'Music for a Rainy Afternoon', 'rainy']);
  const hot = momentOf({ condition: 'sunny', cloud: 0, temperature: 35, day: true }, 15);
  assert.deepEqual([hot.name, hot.sky], ['Music for Hot Weather', 'hot']);
  const night = momentOf({ condition: 'clear-night', cloud: 0, temperature: 12, day: false }, 0);
  assert.deepEqual([night.name, night.sky, night.dark], ['Music for a Clear Night', 'clear', true]);
  assert.equal(momentOf({ condition: 'partlycloudy', cloud: 40, temperature: 17, day: true }, 11).name, 'Music for a Few Clouds');
});

test('wet is never hot, and a sunny word after dark is a clear one', () => {
  assert.equal(momentOf({ condition: 'pouring', temperature: 30, day: true }, 15).name, 'Music for a Rainy Afternoon');
  assert.equal(momentOf({ condition: 'sunny', temperature: 15, day: false }, 23.5).name, 'Music for a Clear Night');
  assert.equal(momentOf({ condition: 'sunny', temperature: 15, day: false }, 21).sky, 'clear');
});

test('every sky Home paints is reached, by the word Cratarr gives it', () => {
  const sky = (condition: string) => momentOf({ condition, day: true }, 12).sky;
  assert.deepEqual(['cloudy', 'fog', 'rainy', 'pouring', 'lightning', 'lightning-rainy', 'hail', 'snowy', 'snowy-rainy', 'windy', 'windy-variant', 'exceptional'].map(sky), [
    'cloudy',
    'foggy',
    'rainy',
    'rainy',
    'stormy',
    'stormy',
    'stormy',
    'snowy',
    'snowy',
    'windy',
    'windy',
    'plain',
  ]);
});

test('each of WMO\'s codes reads as the word Home Assistant has for it', () => {
  const by: Record<string, number[]> = {
    sunny: [0, 1],
    partlycloudy: [2],
    cloudy: [3],
    fog: [45, 48],
    rainy: [51, 53, 55, 56, 57, 61, 63, 80, 81],
    pouring: [65, 82],
    'snowy-rainy': [66, 67],
    snowy: [71, 73, 75, 77, 85, 86],
    'lightning-rainy': [95],
    hail: [96, 99],
  };
  for (const [word, codes] of Object.entries(by)) for (const c of codes) assert.equal(conditionOf(c, true), word, `code ${c}`);
  assert.equal(conditionOf(0, false), 'clear-night');
  assert.equal(conditionOf(1, false), 'clear-night');
  assert.equal(conditionOf(4, true), null, 'no such code');
});

test("Open-Meteo's answer is read, and what is no answer is none", () => {
  assert.deepEqual(weatherOf({ current: { weather_code: 61, cloud_cover: 90, temperature_2m: 9.5, is_day: 0 } }), { condition: 'rainy', cloud: 90, temperature: 9.5, day: false });
  assert.equal(weatherOf({ current: { temperature_2m: 9 } }), null);
  assert.equal(weatherOf({}), null);
  assert.equal(weatherOf(null), null);
});

test('a place goes out to one decimal, whatever is given', () => {
  assert.equal(coarse(52.08371), 52.1);
  assert.equal(coarse(-5.449), -5.4);
  const url = new URL(askFor(52.08371, 5.11782));
  assert.equal(url.host, 'api.open-meteo.com');
  assert.equal(url.searchParams.get('latitude'), '52.1');
  assert.equal(url.searchParams.get('longitude'), '5.1');
  assert.deepEqual(url.searchParams.get('current')?.split(','), ['weather_code', 'cloud_cover', 'temperature_2m', 'is_day']);
});
