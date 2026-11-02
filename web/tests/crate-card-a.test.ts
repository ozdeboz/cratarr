// The crate card's changes and the glass's field (0.13; the owner, 2026-10-31): a term left out and taken back in
// (lib/quickCrate leftOut), the field's dimmed joining words (dimmedIn), and the results by what the terms find
// (answerOfTerms).
import { test } from 'node:test';
import assert from 'node:assert/strict';
await import('./with-matcher.ts');

const { canLeaveOut, leftOut, dimmedIn, answerOfTerms, conditionsOf, sentenceOf, chipOf } = await import('../src/lib/quickCrate.ts');

const genre = (v: string) => ({ kind: 'genre' as const, value: v, label: v });
const artist = (v: string, id: string) => ({ kind: 'artist' as const, value: v, label: v, id });
const year = (v: string, label: string) => ({ kind: 'year' as const, value: v, label });

test('Leave out: a genre becomes "but not", a year too; a track cannot be left out', () => {
  const jazz = chipOf('jazz', [genre('Jazz')])!;
  const hipHop = chipOf('hip hop', [genre('Hip Hop')])!;
  assert.equal(canLeaveOut(jazz), true);
  assert.equal(canLeaveOut(chipOf('90s', [year('1990-1999', '1990s')])!), true);
  const out = leftOut(jazz, true);
  assert.equal(out.not, true);
  assert.deepEqual(
    conditionsOf([hipHop, out]).map((c) => [c.axis, c.not, c.value]),
    [
      ['genre', false, { kind: 'any_of', values: ['Hip Hop'] }],
      ['genre', true, { kind: 'any_of', values: ['Jazz'] }],
    ],
  );
  assert.equal(
    sentenceOf([hipHop, out])
      .map((p) => p.text)
      .join(' '),
    'Hip Hop , but not Jazz',
  );
  const years = chipOf('90s', [year('1990-1999', '1990s')])!;
  assert.equal(leftOut(years, true).not, true, 'a year left out too');
  assert.equal(leftOut(jazz, false), jazz, 'already in: unchanged');
});

test('Leave out an artist one sounds like: "but not by" them; taken back in, sounds like them again', () => {
  const miles = chipOf('miles davis', [artist('Miles Davis', 'ar-1')])!;
  assert.equal(miles.how, 'like');
  const out = leftOut(miles, true);
  assert.deepEqual([out.not, out.how], [true, 'by']);
  assert.deepEqual(
    conditionsOf([out]).map((c) => [c.axis, c.not]),
    [['artist', true]],
  );
  const back = leftOut(out, false);
  assert.deepEqual([!!back.not, back.how], [false, 'like']);
});

test('the field dims the joining words typed between the terms, as typed, each run one range', () => {
  const text = 'hip hop and jazz from the 90s like miles davis';
  const at = (r: [number, number][]) => r.map(([a, b]) => text.slice(a, b));
  assert.deepEqual(at(dimmedIn(text, ['hip hop', 'jazz', '90s', 'miles davis'])), ['and', 'from the', 'like']);
  // A term's own words are never dimmed: "The Roots" keeps its "The".
  const roots = 'the roots and hip hop';
  assert.deepEqual(
    dimmedIn(roots, ['the roots', 'hip hop']).map(([a, b]) => roots.slice(a, b)),
    ['and'],
  );
  // "but not", spaces and capitals as typed.
  const not = 'Hip Hop  but  not Jazz';
  assert.deepEqual(
    dimmedIn(not, ['hip hop', 'jazz']).map(([a, b]) => not.slice(a, b)),
    ['but  not'],
  );
  // Plain words, not the sentence's: none.
  assert.deepEqual(dimmedIn('hip hop jazz 90s', ['hip hop', 'jazz', '90s']), []);
});

test("the results by the terms: each term's own answer, a find once, the totals added", () => {
  const hit = (subject: string) => ({ subject });
  const answers = new Map([
    ['hip hop', { q: 'hip hop', groups: { genre: { total: 1, items: [hit('genre:Hip Hop')] }, track: { total: 40, items: [hit('t1'), hit('t2')] } }, top: [] }],
    ['jazz', { q: 'jazz', groups: { genre: { total: 2, items: [hit('genre:Jazz'), hit('genre:Hip Hop')] }, track: { total: 9, items: [hit('t2'), hit('t3')] } }, top: [] }],
    ['miles davis', { q: 'miles davis', groups: { artist: { total: 1, items: [hit('artist:Miles Davis')] } }, top: [] }],
  ]);
  const r = answerOfTerms('hip hop and jazz like miles davis', ['hip hop', 'Jazz', 'miles  davis', 'not asked'], answers);
  assert.deepEqual(r.groups.genre?.items.map((i) => i.subject), ['genre:Hip Hop', 'genre:Jazz']);
  assert.equal(r.groups.genre?.total, 3);
  assert.deepEqual(r.groups.track?.items.map((i) => i.subject), ['t1', 't2', 't3']);
  assert.deepEqual(r.groups.artist?.items.map((i) => i.subject), ['artist:Miles Davis']);
});
