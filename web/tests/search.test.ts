// One search's browser side (lib/search, decision 0072): the words as the server reads them (the shared examples), a
// word's start matching, whole words ranking higher, and why a result is there.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const { words, score, match, matches, matchedOn, relevance, byRelevance } = await import('../src/lib/search.ts');
const examples: { q: string; words: string[] }[] = JSON.parse(fs.readFileSync(new URL('./fixtures/search-words.json', import.meta.url), 'utf8'));
const typoExamples: { q: string; text: string; typos: number | null }[] = JSON.parse(
  fs.readFileSync(new URL('./fixtures/search-typos.json', import.meta.url), 'utf8'),
);

test('the words, as the shared examples have them (the server reads them alike)', () => {
  for (const e of examples) assert.deepEqual(words(e.q), e.words, e.q);
});

test("each word a word's start, every word needed; accents and case aside", () => {
  assert.ok(matches(words('bjork'), 'Björk', 'Homogenic'));
  assert.ok(matches(words('rolling ston'), 'The Rolling Stones'));
  assert.ok(!matches(words('lling'), 'The Rolling Stones'), "not inside a word");
  assert.ok(!matches(words('rolling beatles'), 'The Rolling Stones'), 'every word');
  assert.ok(matches([], 'anything'), 'no words: everything');
});

test('a whole word ranks above a word\'s start', () => {
  assert.ok(score(words('air'), 'Air') > score(words('air'), 'Airbag'));
});

test('typos, as the shared examples have them (the server allows them alike)', () => {
  for (const e of typoExamples) assert.equal(match(words(e.q), e.text)?.typos ?? null, e.typos, `${e.q} / ${e.text}`);
});

test('everywhere\'s order: direct, fewer typos, the exact name, whole words, the start, the most of the name', () => {
  const q = words('player');
  const r = (title: string, matched: { on: string; text: string }[] = []) => ({ kind: 'x', subject: title, title, score: 0, matched });
  const order = (...rs: ReturnType<typeof r>[]) => [...rs].sort((a, b) => byRelevance(relevance(q, a), relevance(q, b))).map((x) => x.title);
  assert.deepEqual(order(r('Player Piano Sessions'), r('Player')), ['Player', 'Player Piano Sessions'], 'the exact name first');
  assert.deepEqual(order(r('Some Artist', [{ on: 'album', text: 'Player' }]), r('The Player Piano')), ['The Player Piano', 'Some Artist'], 'direct above deep');
  assert.deepEqual(order(r('Plyer'), r('The Player Piano')), ['The Player Piano', 'Plyer'], 'no typos above typos');
  assert.deepEqual(order(r('The Player Piano'), r('Player Piano')), ['Player Piano', 'The Player Piano'], 'the start');
  assert.deepEqual(order(r('Players Club'), r('Late Player')), ['Late Player', 'Players Club'], 'a whole word above a start');
});

test('why it is there: the fields holding the words', () => {
  const m = matchedOn(words('teardrop'), [
    { on: 'name', text: 'Late Night' },
    { on: 'track', text: 'Teardrop' },
  ]);
  assert.deepEqual(m, [{ on: 'track', text: 'Teardrop' }]);
});
