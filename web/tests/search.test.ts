// One search's browser side (lib/search, decision 0072): the words as the server reads them (the shared examples), a
// word's start matching, whole words ranking higher, and why a result is there.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const { words, score, matches, matchedOn } = await import('../src/lib/search.ts');
const examples: { q: string; words: string[] }[] = JSON.parse(fs.readFileSync(new URL('./fixtures/search-words.json', import.meta.url), 'utf8'));

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

test('why it is there: the fields holding the words', () => {
  const m = matchedOn(words('teardrop'), [
    { on: 'name', text: 'Late Night' },
    { on: 'track', text: 'Teardrop' },
  ]);
  assert.deepEqual(m, [{ on: 'track', text: 'Teardrop' }]);
});
