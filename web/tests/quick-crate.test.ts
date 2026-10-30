// Quick crates (lib/quickCrate, 0.11): the terms typed, a year or a decade read by rule, a term's best reading among
// search's kinds and its others, and the chips as a crate's conditions (filtered first, then Sounds like, cut).
import { test } from 'node:test';
import assert from 'node:assert/strict';
await import('./with-matcher.ts');

const { splitTerms, yearOf, readingOf, readingsFor, chipOf, readAs, seedsOf, conditionsOf, nameOf, countAt, minAt, fits, readLine, sentenceOf, recentFor, likeChip, wordChip, crateOfLine, CLOSE, LIKE_MOST } = await import(
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
  // The matcher (0.12): run together, or with a typo, still the name; a word too many is not.
  assert.equal(fits('hiphop', [hit('genre', 'Hip Hop')]), true, 'run together');
  assert.equal(fits('robin trip', [hit('artist', 'Robin Trio')]), true, 'a typo in a name of its length');
  assert.equal(fits('jazz funk soul', [hit('style', 'Jazz-Funk')]), false, 'a word too many');
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

// The crate as a sentence (0.12, the crate search's mockup B): what it says is what the conditions do.
const said = (parts: { text: string; chip?: number; tight?: boolean }[]) => parts.map((p, k) => (k && !p.tight ? ' ' : '') + (p.chip !== undefined ? `[${p.text}]` : p.text)).join('');
const c = (kind: string, label: string, more: Record<string, unknown> = {}) => ({
  term: label.toLowerCase(),
  reading: { kind, value: kind === 'year' ? (more.value as string) : label, label, ...(more.id ? { id: more.id } : {}) },
  others: [],
  ...(kind === 'artist' ? { how: more.how ?? 'like', own: false } : {}),
});
const decade = (d: number) => c('year', `${d}s`, { value: `${d}-${d + 9}` });
const year = (y: number) => c('year', String(y), { value: `${y}-${y}` });

test("the sentence: the mockup's crate; each term its chip's place", () => {
  const chips = [c('genre', 'Hip Hop'), decade(1990), c('artist', 'Miles Davis', { id: 'ar-1' })];
  const parts = sentenceOf(chips);
  assert.equal(said(parts), '[Hip Hop] from the [1990s] that sounds like [Miles Davis]');
  assert.deepEqual(parts.filter((p) => p.chip !== undefined).map((p) => p.chip), [0, 1, 2]);
});

test("the sentence: each kind's terms any of them (or), likes all of them (and), no genre: Tracks with plural verbs", () => {
  assert.equal(said(sentenceOf([c('genre', 'Jazz'), c('genre', 'Soul'), c('genre', 'Funk')])), '[Jazz], [Soul] or [Funk]');
  assert.equal(
    said(sentenceOf([c('artist', 'Bonobo', { id: 'ar-2' }), c('artist', 'Floating Points', { id: 'ar-3' }), c('mood', 'Calm')])),
    'Tracks that feel [Calm] and sound like [Bonobo] and [Floating Points]',
  );
  assert.equal(said(sentenceOf([c('artist', 'Nina Simone', { how: 'by' })])), 'Tracks by [Nina Simone]');
});

test('the sentence: style beside a genre, tags, label, album; the years one span', () => {
  assert.equal(
    said(sentenceOf([c('genre', 'Hip Hop'), c('style', 'Boom Bap'), c('tag', 'Sampled'), c('label', 'Blue Note'), c('album', 'Kind of Blue')])),
    '[Hip Hop], style [Boom Bap] tagged [Sampled] on [Blue Note] from the album [Kind of Blue]',
  );
  assert.equal(said(sentenceOf([c('style', 'Jazz-Funk'), year(1975)])), '[Jazz-Funk] from [1975]');
  assert.equal(said(sentenceOf([c('genre', 'Soul'), year(1985), decade(1970)])), '[Soul] from the [1970s] to [1985]');
  assert.equal(said(sentenceOf([c('genre', 'Soul'), c('year', '1970–1985', { value: '1970-1985' })])), '[Soul] from [1970 to 1985]');
  assert.equal(said(sentenceOf([decade(1960), year(1975), decade(1980)])), 'Tracks from the [1960s], [1975] to the [1980s]');
});

// One way to make a crate (0.12): a menu's "Sounds like this" hands over a track, an album or an artist as the "like"
// term, a word's crate its word; global search's words make a crate card only where they read as two kinds or more.
test('Sounds like this: a track or an album as the like term, by its id; never its own axis', () => {
  const track = likeChip('track', 'tr-7', 'Rebirth of Slick');
  assert.deepEqual(seedsOf([track]), ['tr-7']);
  assert.deepEqual(conditionsOf([track]), [{ axis: 'like', not: false, value: { kind: 'like', of: ['tr-7'], count: LIKE_MOST, min: CLOSE } }]);
  assert.equal(said(sentenceOf([track])), 'Tracks that sound like [Rebirth of Slick]');
  const album = likeChip('album', 'al-3', 'Kind of Blue');
  const rows = conditionsOf([c('genre', 'Jazz'), album]);
  assert.deepEqual(rows.map((r) => r.axis), ['genre', 'like']);
  assert.equal(said(sentenceOf([c('genre', 'Jazz'), album])), '[Jazz] that sounds like [Kind of Blue]');
  const artist = likeChip('artist', 'ar-9', 'Miles Davis');
  assert.equal(artist.reading.value, 'Miles Davis');
  assert.deepEqual(seedsOf([artist]), ['ar-9']);
});

test("Grab a crate from a word's crate: that word as its term", () => {
  const label = wordChip('label', 'Blue Note');
  assert.deepEqual(conditionsOf([label]), [{ axis: 'label', not: false, value: { kind: 'any_of', values: ['Blue Note'] } }]);
  assert.equal(said(sentenceOf([label])), 'Tracks on [Blue Note]');
  assert.equal(said(sentenceOf([wordChip('genre', 'Soul')])), '[Soul]');
});

test("global search's crate card: two terms or more of two kinds or more; one thing stays the plain row", () => {
  const r = (term: string, ...readings: Record<string, unknown>[]) => ({ term, readings });
  const genre = (v: string) => ({ kind: 'genre', value: v, label: v });
  const artist = (v: string, id: string) => ({ kind: 'artist', value: v, label: v, id });
  const nineties = { kind: 'year', value: '1990-1999', label: '1990s' };
  const chips = crateOfLine([r('hip hop', genre('Hip Hop')), r('90s', nineties), r('miles davis', artist('Miles Davis', 'ar-1'))]);
  assert.ok(chips);
  assert.equal(said(sentenceOf(chips!)), '[Hip Hop] from the [1990s] that sounds like [Miles Davis]');
  assert.equal(crateOfLine([r('miles davis', artist('Miles Davis', 'ar-1'))]), null, 'one name');
  assert.equal(crateOfLine([r('jazz', genre('Jazz')), r('soul', genre('Soul'))]), null, 'one kind');
  assert.equal(crateOfLine([r('jazz', genre('Jazz')), r('qwzx')]), null, 'a word read as nothing leaves one term');
  assert.ok(crateOfLine([r('jazz', genre('Jazz')), r('qwzx'), r('1975', { kind: 'year', value: '1975-1975', label: '1975' })]), 'what reads still makes it');
});

test('the crates made here and not saved show only once typing starts, those the words may mean', () => {
  const chip = (term: string, kind: string, more: Record<string, unknown> = {}) =>
    ({ term, reading: { kind, value: term, label: term, ...(more.id ? { id: more.id } : {}) }, others: [], ...more }) as never;
  const soul = { chips: [chip('Soul', 'genre'), chip('1971', 'year'), chip('Stax', 'label')], min: 0 };
  const ambient = { chips: [chip('Ambient', 'genre'), chip('Warp', 'label')], min: 0 };
  const like = { chips: [chip('Jazz', 'genre'), chip('Ann Quartet', 'artist', { how: 'like', id: 'ar-ann' })], min: 60 };
  const recent = [soul, ambient, like];
  assert.deepEqual(recentFor(recent, ''), [], 'nothing typed: the regular page, none shown');
  assert.deepEqual(recentFor(recent, '   '), []);
  assert.deepEqual(recentFor(recent, 's'), [soul], 'the last word as a beginning');
  assert.deepEqual(recentFor(recent, 'wa'), [ambient]);
  assert.deepEqual(recentFor(recent, 'ambient w'), [ambient], 'every word, the last a beginning');
  assert.deepEqual(recentFor(recent, 'like ann'), [like], "a like's own word counts");
  assert.deepEqual(recentFor(recent, 'zz'), [], 'a later word inside a name only when not the last');
  assert.deepEqual(recentFor(recent, 'jazz, an'), [like], 'commas as typed in the field');
});
