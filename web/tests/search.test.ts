// One search's browser side (lib/search, decision 0072; 0.12: the server's matcher, lib/matcher): the words as the server
// reads them (the shared examples), what matches, the order everywhere's results take, and why a result is there.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const { matcher } = await import('./with-matcher.ts');
const { words, score, matches, matchedOn, relevance, byRelevance, useMatcher } = await import('../src/lib/search.ts');
const examples: { q: string; words: string[] }[] = JSON.parse(fs.readFileSync(new URL('./fixtures/search-words.json', import.meta.url), 'utf8'));

test('the words, as the shared examples have them (the server reads them alike)', () => {
  for (const e of examples) assert.deepEqual(words(e.q), e.words, e.q);
});

test('what matches: accents and case aside, separators aside, a start, a few typos; not a stranger', () => {
  assert.ok(matches(words('bjork'), 'Björk', 'Homogenic'));
  assert.ok(matches(words('rolling ston'), 'The Rolling Stones'));
  assert.ok(matches(words('hiphop'), 'Hip Hop'));
  assert.ok(matches(words('radiohed'), 'Radiohead'));
  assert.ok(!matches(words('hiphop'), 'Trip Hop'));
  assert.ok(!matches(words('beatles'), 'The Rolling Stones'));
  assert.ok(matches([], 'anything'), 'no words: everything');
});

test("an item's fields are one name: words across them", () => {
  assert.ok(matches(words('kind blue miles'), 'Kind of Blue', 'Miles Davis'));
  assert.ok(matches(words('miles davis'), 'Kind of Blue', 'Miles Davis'));
});

test('a whole name ranks above its start', () => {
  assert.ok(score(words('air'), 'Air') > score(words('air'), 'Airbag'));
});

test("everywhere's order: direct, no guess, the exact name, the stronger meeting, the start, the most of the name", () => {
  const q = words('player');
  const r = (title: string, matched: { on: string; text: string }[] = []) => ({ kind: 'x', subject: title, title, score: 0, matched });
  const order = (...rs: ReturnType<typeof r>[]) => [...rs].sort((a, b) => byRelevance(relevance(q, a), relevance(q, b))).map((x) => x.title);
  assert.deepEqual(order(r('Player Piano Sessions'), r('Player')), ['Player', 'Player Piano Sessions'], 'the exact name first');
  assert.deepEqual(order(r('Some Artist', [{ on: 'album', text: 'Player' }]), r('The Player Piano')), ['The Player Piano', 'Some Artist'], 'direct above deep');
  assert.deepEqual(order(r('Plyer'), r('The Player Piano')), ['The Player Piano', 'Plyer'], 'no guess above a guess');
  assert.deepEqual(order(r('The Player Piano'), r('Player Piano')), ['Player Piano', 'The Player Piano'], 'the start');
  assert.deepEqual(order(r(' Hip-Hop'), r('Hip Hop Classics')), [' Hip-Hop', 'Hip Hop Classics'], 'separators aside, exact');
  assert.deepEqual(order(r('Players Club'), r('Late Player')), ['Late Player', 'Players Club'], 'a whole word above a start');
});

test('why it is there: the fields holding the words', () => {
  const m = matchedOn(words('teardrop'), [
    { on: 'name', text: 'Late Night' },
    { on: 'track', text: 'Teardrop' },
  ]);
  assert.deepEqual(m, [{ on: 'track', text: 'Teardrop' }]);
});

test('before the matcher is in, a search leaves nothing out', () => {
  useMatcher(null);
  try {
    assert.ok(matches(words('anything at all'), 'Teardrop'));
  } finally {
    useMatcher(matcher);
  }
});
