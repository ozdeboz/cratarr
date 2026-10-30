// Quick crates (lib/quickCrate, 0.11): the terms typed, a year or a decade read by rule, a term's best reading among
// search's kinds and its others, and the chips as a crate's conditions (filtered first, then Sounds like, cut).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { splitTerms, yearOf, readingOf, readingsFor, chipOf, readAs, seedsOf, conditionsOf, nameOf, countAt, minAt, fits, readLine, CLOSE, LIKE_MOST } = await import(
  '../src/lib/quickCrate.ts'
);
const { words, match } = await import('../src/lib/search.ts');

const hit = (kind: string, title: string, more: Record<string, unknown> = {}) => ({
  kind,
  subject: kind === 'album' ? `album:cratarr/al-${title.toLowerCase()}` : `${kind}:${title}`,
  title,
  score: 1,
  matched: [],
  ...more,
});

test('the terms: apart at commas, semicolons and lines, trimmed, each once', () => {
  assert.deepEqual(splitTerms(' jazz,  Bonobo ;1970s\nsoul , Jazz,,'), ['jazz', 'Bonobo', '1970s', 'soul']);
  assert.deepEqual(splitTerms('  deep   house '), ['deep house']);
  assert.deepEqual(splitTerms(' , ; '), []);
});

test('a year and a decade by rule, a two-digit one in this century up to this year', () => {
  const now = 2026;
  const span = (t: string) => yearOf(t, now)?.value ?? null;
  assert.equal(span('1975'), '1975-1975');
  assert.equal(span("'75"), '1975-1975');
  assert.equal(span('’75'), '1975-1975');
  assert.equal(span("'05"), '2005-2005');
  assert.equal(span('1970s'), '1970-1979');
  assert.equal(span("1970's"), '1970-1979');
  assert.equal(span('70s'), '1970-1979');
  assert.equal(span("'70s"), '1970-1979');
  assert.equal(span('20s'), '2020-2029');
  assert.equal(span('30s'), '1930-1939');
  assert.equal(span('1979-1970'), '1970-1979');
  assert.equal(yearOf('70s', now)?.label, '1970s');
  for (const t of ['75', '1975s', 'abba', '197', '12345', '0999']) assert.equal(yearOf(t, now), null, t);
});

test('a hit as a reading: an artist with its id, an album by its id, a name for the rest, none for a track', () => {
  assert.deepEqual(readingOf(hit('artist', 'Bonobo', { id: 'ar-1' })), { kind: 'artist', value: 'Bonobo', label: 'Bonobo', id: 'ar-1' });
  assert.deepEqual(readingOf(hit('artist', 'Bonobo')), { kind: 'artist', value: 'Bonobo', label: 'Bonobo' });
  assert.deepEqual(readingOf(hit('album', 'Kind')), { kind: 'album', value: 'al-kind', label: 'Kind' });
  assert.deepEqual(readingOf(hit('label', 'Ninja Tune')), { kind: 'label', value: 'Ninja Tune', label: 'Ninja Tune' });
  assert.equal(readingOf(hit('track', 'Kiara')), null);
  assert.equal(readingOf(hit('crate', 'Mine')), null);
});

test("a term's readings: the closer name first, equals a genre before an artist, a year first, each once", () => {
  const soul = readingsFor('soul', [hit('artist', 'Soul', { id: 'ar-s' }), hit('genre', 'Soul'), hit('style', 'Neo Soul'), hit('track', 'Soul')]);
  assert.deepEqual(
    soul.map((r) => `${r.kind}:${r.value}`),
    ['genre:Soul', 'artist:Soul', 'style:Neo Soul'],
  );
  const air = readingsFor('air', [hit('mood', 'Airy'), hit('artist', 'Air', { id: 'ar-air' })]);
  assert.equal(air[0].kind, 'artist', 'the whole name before a longer one');
  const year = readingsFor('1975', [hit('album', '1975'), hit('album', '1975')], 2026);
  assert.deepEqual(
    year.map((r) => r.kind),
    ['year', 'album'],
  );
  assert.deepEqual(readingsFor('zzz', []), []);
});

test('a chip: an artist sounds like where it has an id, by them where not; another reading a tap away', () => {
  const r = readingsFor('soul', [hit('genre', 'Soul'), hit('artist', 'Soul', { id: 'ar-s' })]);
  const chip = chipOf('soul', r)!;
  assert.equal(chip.reading.kind, 'genre');
  assert.equal(chip.how, undefined);
  const asArtist = readAs(chip, chip.others[0]);
  assert.equal(asArtist.reading.kind, 'artist');
  assert.equal(asArtist.how, 'like');
  assert.equal(asArtist.own, false);
  assert.deepEqual(
    asArtist.others.map((o) => o.kind),
    ['genre'],
  );
  assert.equal(readAs(asArtist, asArtist.others[0]).how, undefined);
  assert.equal(chipOf('x', [{ kind: 'artist', value: 'X', label: 'X' }])!.how, 'by');
  assert.equal(chipOf('x', []), null);
});

test('the conditions: each kind one row, any of them; the years one span; by an artist a row; like them one Sounds like', () => {
  const chip = (term: string, kind: string, value: string, more: Record<string, unknown> = {}) =>
    ({ term, reading: { kind, value, label: term, ...(more.id ? { id: more.id } : {}) }, others: [], ...more }) as never;
  const chips = [
    chip('jazz', 'genre', 'Jazz'),
    chip('funk', 'genre', 'Funk'),
    chip('1970s', 'year', '1970-1979'),
    chip('1985', 'year', '1985-1985'),
    chip('Bonobo', 'artist', 'Bonobo', { id: 'ar-b', how: 'like', own: false }),
    chip('Air', 'artist', 'Air', { id: 'ar-a', how: 'by', own: false }),
    chip('Kind', 'album', 'al-kind'),
  ];
  assert.deepEqual(conditionsOf(chips), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Jazz', 'Funk'] } },
    { axis: 'artist', not: false, value: { kind: 'any_of', values: ['Air'] } },
    { axis: 'album', not: false, value: { kind: 'any_of', values: ['al-kind'] } },
    { axis: 'year', not: false, value: { kind: 'range', min: 1970, max: 1985 } },
    { axis: 'like', not: false, value: { kind: 'like', of: ['ar-b'], count: LIKE_MOST, min: CLOSE } },
  ]);
  assert.deepEqual(seedsOf(chips), ['ar-b']);
  const kept = conditionsOf([chip('Bonobo', 'artist', 'Bonobo', { id: 'ar-b', how: 'like', own: true })], 40);
  assert.deepEqual(kept, [{ axis: 'like', not: false, value: { kind: 'like', of: ['ar-b'], count: LIKE_MOST, min: 40, own: true } }]);
  assert.deepEqual(conditionsOf([]), []);
  assert.equal(nameOf(chips), 'jazz · funk · 1970s · 1985 · like Bonobo · Air · Kind');
});

test('the cut on the closeness curve: how many at least that alike, and back', () => {
  const curve = [98, 90, 75, 61, 60, 59, 40];
  assert.equal(countAt(curve, 60), 5);
  assert.equal(countAt(curve, 99), 0);
  assert.equal(minAt(curve, 5), 60);
  assert.equal(minAt(curve, 3), 75);
  assert.equal(minAt(curve, 0), 0);
  assert.equal(minAt(curve, 99), 0);
});

// A line read whole (the owner, 2026-10-30: "Read the whole line"). A small library, searched as the server does for
// a phrase: every word of it in a name (lib/search's `match`); `asked` what was asked, to count it.
const LIBRARY = [
  hit('genre', 'Hip Hop'),
  hit('style', 'Hip Hop Soul'),
  hit('genre', 'Jazz'),
  hit('genre', 'Funk'),
  hit('style', 'Jazz-Funk'),
  hit('style', 'Neo Soul'),
  hit('artist', 'Ann Quartet', { id: 'ar-q' }),
  hit('artist', 'Robin Trio', { id: 'ar-r' }),
  hit('artist', 'Robin', { id: 'ar-rob' }),
  hit('album', 'Kind of Blue'),
  hit('album', 'The Dark Side of the Moon'),
];
function library() {
  const asked: string[] = [];
  const ask = async (phrase: string) => {
    asked.push(phrase);
    return LIBRARY.filter((h) => match(words(phrase), h.title));
  };
  return { ask, asked };
}
const read = async (line: string) => {
  const lib = library();
  const out = await readLine(line, lib.ask, 2026);
  return { terms: out.map((r) => `${r.term} → ${r.readings[0] ? `${r.readings[0].kind}:${r.readings[0].label}` : 'nothing'}`), asked: lib.asked };
};

test('a phrase fits a hit whose own name it is, give or take a few letters, not a longer name it is part of', () => {
  assert.equal(fits('hip hop', [hit('genre', 'Hip Hop')]), true);
  assert.equal(fits('hip ho', [hit('genre', 'Hip Hop')]), true, 'a word still being typed');
  assert.equal(fits('robin', [hit('artist', 'Robin Trio')]), false, 'part of a longer name');
  assert.equal(fits('soul', [hit('style', 'Neo Soul')]), false);
  assert.equal(fits('robin', [hit('track', 'Robin')]), false, 'a track is no reading');
  assert.equal(fits('zzz', []), false);
});

test('a line: years and decades first, then the longest names that fit, in the order typed; one search each', async () => {
  const r = await read('hip hop 90s ann quartet');
  assert.deepEqual(r.terms, ['hip hop → genre:Hip Hop', '90s → year:1990s', 'ann quartet → artist:Ann Quartet']);
  assert.deepEqual(r.asked, ['hip hop', 'ann quartet'], 'a year asks nothing; each name one search');
});

test('words that make no one name: the longest first, then shorter together, each word its own where nothing longer fits', async () => {
  const r = await read('jazz funk soul');
  assert.deepEqual(r.terms, ['jazz funk → style:Jazz-Funk', 'soul → style:Neo Soul']);
  assert.deepEqual((await read('funk jazz')).terms, ['funk jazz → style:Jazz-Funk'], "a name's words in any order");
  const two = await read('funk soul robin trio');
  assert.deepEqual(two.terms, ['funk → genre:Funk', 'soul → style:Neo Soul', 'robin trio → artist:Robin Trio']);
  assert.ok(two.asked.length <= 8, `asked ${two.asked.length}: ${two.asked.join(' | ')}`);
  assert.equal(new Set(two.asked.map((a) => a.toLowerCase())).size, two.asked.length, 'no phrase asked twice');
});

test('a long name read whole; a comma still splits; a span of years one term; nothing found stays a word of its own', async () => {
  assert.deepEqual((await read('the dark side of the moon')).terms, ['the dark side of the moon → album:The Dark Side of the Moon']);
  assert.deepEqual((await read('the dark side of the moon')).asked.length, 1);
  assert.deepEqual((await read('kind of blue, 1959')).terms, ['kind of blue → album:Kind of Blue', '1959 → year:1959']);
  assert.deepEqual((await read('jazz 1970 - 1979')).terms, ['jazz → genre:Jazz', '1970-1979 → year:1970–1979']);
  assert.deepEqual((await read('jazz zzz')).terms, ['jazz → genre:Jazz', 'zzz → nothing']);
  assert.deepEqual((await read('jazz, Jazz, - ,')).terms, ['jazz → genre:Jazz']);
});

test('a search that fails reads as nothing found, the rest of the line still read', async () => {
  const out = await readLine('jazz, robin', async (p) => (p === 'jazz' ? Promise.reject(new Error('down')) : LIBRARY.filter((h) => match(words(p), h.title))), 2026);
  assert.deepEqual(
    out.map((r) => [r.term, r.readings[0]?.label ?? null]),
    [
      ['jazz', null],
      ['robin', 'Robin'],
    ],
  );
});
