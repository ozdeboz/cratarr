// Quick crates (lib/quickCrate, 0.11): the terms typed, a year or a decade read by rule, a term's best reading among
// search's kinds and its others, and the chips as a crate's conditions (filtered first, then Sounds like, cut).
import { test } from 'node:test';
import assert from 'node:assert/strict';
await import('./with-matcher.ts');

const { splitTerms, yearOf, tempoOf, tempoSaid, readingOf, readingsFor, chipOf, readAs, seedsOf, conditionsOf, nameOf, countAt, minAt, fits, readLine, sentenceOf, recentFor, likeChip, wordChip, crateOfLine, CLOSE, LIKE_MOST, chipsOf, modesFrom, joinOf, briefOf, keptChips, namesOf, crateShows, crateCardOn, readOnto } = await import(
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

test('the cut is a whole number, as the server takes it, rounded down so the track at the handle stays in', () => {
  const curve = [97.6, 61.9, 25.37, 0.4];
  assert.equal(minAt(curve, 3), 25);
  assert.ok(Number.isInteger(minAt(curve, 2)));
  assert.ok(countAt(curve, minAt(curve, 3)) >= 3);
  assert.equal(minAt(curve, 4), 0);
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

test("a search's crate card: two terms or more of two kinds or more; one thing makes no crate", () => {
  const r = (term: string, ...readings: Record<string, unknown>[]) => ({ term, readings });
  const genre = (v: string) => ({ kind: 'genre', value: v, label: v });
  const artist = (v: string, id: string) => ({ kind: 'artist', value: v, label: v, id });
  const nineties = { kind: 'year', value: '1990-1999', label: '1990s' };
  const chips = crateOfLine([r('hip hop', genre('Hip Hop')), r('90s', nineties), r('miles davis', artist('Miles Davis', 'ar-1'))]);
  assert.ok(chips);
  assert.equal(said(sentenceOf(chips!)), '[Hip Hop] from the [1990s] that sounds like [Miles Davis]');
  assert.equal(crateOfLine([r('miles davis', artist('Miles Davis', 'ar-1'))]), null, 'one name');
  assert.equal(crateOfLine([r('jazz', genre('Jazz')), r('soul', genre('Soul'))]), null, 'one kind');
  // The owner, 2026-10-31: "jazz or blues… should also result in a crate": one kind joined by a typed "or" or "and".
  assert.ok(crateOfLine([r('jazz', genre('Jazz')), { ...r('blues', genre('Blues')), join: 'or' }]), 'jazz or blues');
  assert.ok(crateOfLine([r('jazz', genre('Jazz')), { ...r('blues', genre('Blues')), join: 'and' }]), 'jazz and blues');
  assert.equal(crateOfLine([r('jazz', genre('Jazz')), r('qwzx')]), null, 'a word read as nothing leaves one term');
  assert.ok(crateOfLine([r('jazz', genre('Jazz')), r('qwzx'), r('1975', { kind: 'year', value: '1975-1975', label: '1975' })]), 'what reads still makes it');
  // The owner, 2026-10-31: "jazz and blues, but not techno" should also work: a term left out makes it a crate.
  const left = crateOfLine([r('jazz', genre('Jazz')), r('blues', genre('Blues')), { ...r('techno', genre('Techno')), not: true }]);
  assert.deepEqual(left?.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Jazz', 'Blues', 'Techno!']);
  assert.ok(crateOfLine([r('hip hop', genre('Hip Hop')), { ...r('trap', genre('Trap')), not: true }]), 'one kind, one left out');
  assert.equal(crateOfLine([{ ...r('trap', genre('Trap')), not: true }]), null, 'only what is left out: no crate');
});

test('said otherwise: "without", "except", "no" leave out; "before", "after", "since", "until" open a span; decades in words and decade to decade', async () => {
  const say = async (line: string) => plain(sentenceOf(await chipsOfLine(line)));
  for (const w of ['without', 'except', 'excluding', 'no']) assert.equal(await say(`hip hop ${w} trap`), 'Hip Hop, but not Trap', w);
  assert.equal(await say('jazz before 1970'), 'Jazz before 1970');
  assert.equal(await say('jazz after the 90s'), 'Jazz after the 1990s');
  assert.equal(await say('jazz since 2010'), 'Jazz since 2010');
  assert.equal(await say('jazz until 1980'), 'Jazz until 1980');
  assert.deepEqual(conditionsOf(await chipsOfLine('jazz before 1970'))[1], { axis: 'year', not: false, value: { kind: 'range', min: null, max: 1969 } });
  assert.deepEqual(conditionsOf(await chipsOfLine('jazz after the 90s'))[1], { axis: 'year', not: false, value: { kind: 'range', min: 2000, max: null } });
  assert.deepEqual(conditionsOf(await chipsOfLine('soul not before 1970'))[1], { axis: 'year', not: true, value: { kind: 'range', min: null, max: 1969 } });
  assert.equal(await say('jazz seventies'), 'Jazz from the 1970s');
  assert.equal(await say('jazz 70s-80s'), 'Jazz from 1970 to 1989');
  assert.equal(await say('jazz 1970s-80s'), 'Jazz from 1970 to 1989');
  // Read back from the conditions, an open span says itself (Filter's open range: no longer one the sentence can't say).
  for (const line of ['jazz before 1970', 'jazz since 2010', 'soul not before 1970']) {
    const conditions = conditionsOf(await chipsOfLine(line));
    const back = chipsOf(conditions);
    assert.deepEqual(back.rest, [], line);
    assert.deepEqual(conditionsOf(back.chips, back.min, back.modes), conditions, line);
  }
  assert.equal(plain(sentenceOf(chipsOf(conditionsOf(await chipsOfLine('jazz before 1970'))).chips)), 'Jazz before 1970');
});

// The sentence's own words typed back (0.12.x; the owner, 2026-10-31: "and" did not work): joins dropped where no name
// holds them, kind words making what follows that kind, and whatever sentenceOf writes reading back to its terms.
const WORDS_LIBRARY = [
  hit('genre', 'Hip Hop'),
  hit('genre', 'Trap'),
  hit('label', 'Def Jam'),
  hit('genre', 'Jazz'),
  hit('genre', 'Soul'),
  hit('genre', 'Funk'),
  hit('genre', 'Blues'),
  hit('genre', 'Techno'),
  hit('style', 'Boom Bap'),
  hit('tag', 'Sampled'),
  hit('mood', 'Calm'),
  hit('label', 'Blue Note'),
  hit('artist', 'Miles Davis', { id: 'ar-md' }),
  hit('artist', 'Al Green', { id: 'ar-ag' }),
  hit('artist', 'The Roots', { id: 'ar-tr' }),
  hit('artist', 'Earth, Wind & Fire', { id: 'ar-ewf' }),
  hit('album', 'Like a Rolling Stone'),
  hit('album', 'Kind of Blue'),
];
const typedBack = async (line: string) => {
  const asked: string[] = [];
  const reads = await readLine(line, async (p) => (asked.push(p), WORDS_LIBRARY.filter((h) => match(words(p), h.title))), 2026);
  const chips = reads.flatMap((r) => chipOf(r.term, r.readings, r.how, r.not) ?? []);
  return { chips, terms: chips.map((ch) => `${ch.reading.kind}:${ch.reading.label}${ch.how ? `/${ch.how}` : ''}`), asked };
};

test("the sentence's joins: and, or, the, from dropped between terms; kept where a name holds them", async () => {
  assert.deepEqual((await typedBack('hip hop and jazz from the 90s')).terms, ['genre:Hip Hop', 'genre:Jazz', 'year:1990s']);
  assert.deepEqual((await typedBack('jazz or soul')).terms, ['genre:Jazz', 'genre:Soul']);
  assert.deepEqual((await typedBack('The Roots')).terms, ['artist:The Roots/like']);
  assert.deepEqual((await typedBack('Earth Wind and Fire')).terms, ['artist:Earth, Wind & Fire/like'], "a name's & typed as and");
  assert.deepEqual((await typedBack('soul from 1970 to 1985')).terms, ['genre:Soul', 'year:1970–1985'], 'a span as the sentence writes it');
});

test("the sentence's kind words: sounds like, by, on, tagged, feels, from the album, style; a name that begins with one stays the name", async () => {
  assert.deepEqual((await typedBack('sounds like Miles Davis on Blue Note')).terms, ['artist:Miles Davis/like', 'label:Blue Note']);
  assert.deepEqual((await typedBack('soul by Al Green')).terms, ['genre:Soul', 'artist:Al Green/by']);
  assert.deepEqual((await typedBack('jazz tagged sampled that feels calm')).terms, ['genre:Jazz', 'tag:Sampled', 'mood:Calm']);
  assert.deepEqual((await typedBack('jazz from the album kind of blue')).terms, ['genre:Jazz', 'album:Kind of Blue']);
  assert.deepEqual((await typedBack('hip hop, style boom bap')).terms, ['genre:Hip Hop', 'style:Boom Bap']);
  assert.deepEqual((await typedBack('like a rolling stone')).terms, ['album:Like a Rolling Stone'], 'not "like" a rolling stone');
  const like = (await typedBack('jazz that sounds like kind of blue')).chips[1];
  assert.equal(like.how, 'like');
  assert.deepEqual(seedsOf([like]), ['al-kind of blue'], 'an album sounded like seeds by its id');
});

test('whatever the sentence writes reads back to the same terms', async () => {
  const crates = [
    [c('genre', 'Hip Hop'), decade(1990), c('artist', 'Miles Davis', { id: 'ar-md' })],
    [c('genre', 'Soul'), c('artist', 'Al Green', { id: 'ar-ag', how: 'by' })],
    [c('genre', 'Jazz'), c('label', 'Blue Note'), c('year', '1970–1985', { value: '1970-1985' })],
    [c('genre', 'Hip Hop'), c('style', 'Boom Bap'), c('tag', 'Sampled'), c('label', 'Blue Note')],
    [c('mood', 'Calm'), c('artist', 'Miles Davis', { id: 'ar-md' }), c('artist', 'Al Green', { id: 'ar-ag' })],
    [c('genre', 'Jazz'), c('genre', 'Soul'), c('genre', 'Funk')],
    [c('genre', 'Soul'), year(1985), decade(1970)],
  ];
  const key = (ch: { reading: { kind: string; label: string }; how?: string }) => `${ch.reading.kind}:${ch.reading.label}${ch.how ? `/${ch.how}` : ''}`;
  for (const chips of crates) {
    const line = sentenceOf(chips as never)
      .map((p, k) => (k && !p.tight ? ' ' : '') + p.text)
      .join('');
    const back = await typedBack(line);
    assert.deepEqual([...back.terms].sort(), chips.map(key).sort(), line);
    assert.deepEqual(conditionsOf(back.chips as never), conditionsOf(chips as never), `${line}: the same crate`);
  }
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

// One crate, two views (0.13, decision 0088): how a kind's terms join, and a crate's conditions read back as its terms.
const reads = (line: string) => readLine(line, async (p) => WORDS_LIBRARY.filter((h) => match(words(p), h.title)), 2026);
const chipsOfLine = async (line: string) => (await reads(line)).flatMap((r) => chipOf(r.term, r.readings, r.how, r.not) ?? []);

test('and or or between two terms of one kind sets how they join; the defaults: Sounds like and, the others or', async () => {
  assert.deepEqual(modesFrom(await reads('jazz and soul')), { genre: 'and' });
  assert.deepEqual(modesFrom(await reads('jazz or soul')), { genre: 'or' });
  assert.deepEqual(modesFrom(await reads('sounds like Miles Davis or Al Green')), { like: 'or' });
  assert.deepEqual(modesFrom(await reads('jazz and the 90s')), {}, 'a year has no choice');
  assert.deepEqual(modesFrom(await reads('and soul'), ['genre']), { genre: 'and' }, 'more typed after a genre already there');
  assert.equal(joinOf({}, 'like'), 'and');
  assert.equal(joinOf({}, 'genre'), 'or');
});

test('the joins in the conditions: all of a kind a track carries several of, Sounds like either; one term stays any of', async () => {
  const genres = await chipsOfLine('jazz and soul');
  assert.deepEqual(conditionsOf(genres, CLOSE, { genre: 'and' })[0].value, { kind: 'all_of', values: ['Jazz', 'Soul'] });
  assert.deepEqual(conditionsOf(genres, CLOSE, {})[0].value, { kind: 'any_of', values: ['Jazz', 'Soul'] });
  assert.deepEqual(conditionsOf(genres.slice(0, 1), CLOSE, { genre: 'and' })[0].value, { kind: 'any_of', values: ['Jazz'] });
  const like = await chipsOfLine('sounds like Miles Davis and Al Green');
  const blend = conditionsOf(like, 40, {}).at(-1)!.value;
  assert.equal(blend.kind === 'like' && blend.mode, undefined, 'blended: no mode');
  const either = conditionsOf(like, 40, { like: 'or' }).at(-1)!.value;
  assert.equal(either.kind === 'like' && either.mode, 'either');
});

test("the sentence says each kind's join, and that word is the one a tap flips", async () => {
  const chips = await chipsOfLine('jazz and soul that sounds like Miles Davis and Al Green');
  const parts = sentenceOf(chips, { genre: 'and', like: 'or' });
  assert.deepEqual(
    parts.filter((p) => p.flip).map((p) => `${p.flip}:${p.text}`),
    ['genre:and', 'like:or'],
  );
  assert.equal(sentenceOf(chips, {}).find((p) => p.flip === 'genre')?.text, 'or');
});

test("a crate's conditions read back as its terms, named from the server's names, and back to the same conditions", async () => {
  for (const [line, modes, min] of [
    ['jazz and soul that sounds like Miles Davis and Al Green', { genre: 'and' as const }, 55],
    ['hip hop from the 90s that sounds like Miles Davis or Al Green', { like: 'or' as const }, 25],
    ['soul by Al Green on Blue Note from 1970 to 1985', {}, CLOSE],
    ['jazz tagged sampled that feels calm', {}, CLOSE],
  ] as const) {
    const chips = await chipsOfLine(line);
    const conditions = conditionsOf(chips, min, modes);
    const names = Object.fromEntries(chips.filter((c) => c.reading.id).map((c) => [c.reading.id!, { name: c.reading.label, kind: c.reading.kind as 'artist' }]));
    const back = chipsOf(conditions, names);
    assert.deepEqual(back.rest, [], line);
    assert.deepEqual(conditionsOf(back.chips, back.min, back.modes), conditions, line);
    assert.deepEqual(new Set(back.chips.map((c) => c.reading.label)), new Set(chips.map((c) => c.reading.label)), line);
  }
});

test('what a sentence cannot say stays as it is (a second row of a kind), an open range said as itself, and an id the library no longer has reads as gone', () => {
  const not = { axis: 'genre', not: true, value: { kind: 'any_of' as const, values: ['Jazz'] } };
  const open = { axis: 'year', not: false, value: { kind: 'range' as const, min: 1990, max: null } };
  const counted = { axis: 'like', not: false, value: { kind: 'like' as const, of: ['ar-x'], count: 50 } };
  const second = { axis: 'genre', not: false, value: { kind: 'any_of' as const, values: ['Soul'] } };
  const first = { axis: 'genre', not: false, value: { kind: 'any_of' as const, values: ['Funk'] } };
  const r = chipsOf([first, not, open, counted, second]);
  // A negated row is "but not"; one by how many is said briefly (both said since the sentence's "but not").
  assert.deepEqual(r.chips.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Funk', 'Jazz!', 'since 1990']);
  assert.deepEqual(r.rest, [second]);
  assert.equal(r.brief?.condition, counted);
  const gone = chipsOf([{ axis: 'like', not: false, value: { kind: 'like', of: ['ar-gone', 'al-x'], count: LIKE_MOST, min: 30, own: true } }]);
  assert.deepEqual(gone.chips.map((c) => c.reading.label), ['an artist no longer in the library', 'an album no longer in the library']);
  assert.equal(gone.chips[0].own, true);
  assert.equal(gone.min, 30);
  assert.equal(gone.modes.like, 'and');
});

// "But not" (0.13): what a crate leaves out, said, typed and read back.
const plain = (parts: { text: string; tight?: boolean }[]) => parts.map((p, k) => (k && !p.tight ? ' ' : '') + p.text).join('');

test('"but not" typed leaves out what follows, through a kind word after it; a "like" ends it', async () => {
  const trap = await chipsOfLine('hip hop but not trap');
  assert.deepEqual(trap.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Hip Hop', 'Trap!']);
  assert.equal(plain(sentenceOf(trap)), 'Hip Hop, but not Trap');
  assert.deepEqual(conditionsOf(trap), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Hip Hop'] } },
    { axis: 'genre', not: true, value: { kind: 'any_of', values: ['Trap'] } },
  ]);
  const label = await chipsOfLine('jazz but not on def jam');
  assert.equal(plain(sentenceOf(label)), 'Jazz, but not on Def Jam');
  const by = await chipsOfLine('soul but not by al green');
  assert.deepEqual(by.map((c) => `${c.reading.kind}:${c.reading.label}/${c.how ?? ''}${c.not ? '!' : ''}`), ['genre:Soul/', 'artist:Al Green/by!']);
  assert.equal(plain(sentenceOf(by)), 'Soul, but not by Al Green');
  const like = await chipsOfLine('jazz but not trap that sounds like miles davis');
  assert.equal(like.at(-1)!.how, 'like');
  assert.equal(like.at(-1)!.not, undefined, 'a like after "but not" is liked, not left out');
});

test('a plain "not" leaves out what follows, a year too; a name never swallows it unless it holds it', async () => {
  // "hip hop not" fit Hip Hop (the matcher forgives a word too many): the "not" was lost, the 2000s went in.
  const decade = await chipsOfLine('hip hop not 00s');
  assert.deepEqual(decade.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Hip Hop', '2000s!']);
  assert.equal(plain(sentenceOf(decade)), 'Hip Hop, but not from the 2000s');
  assert.deepEqual(conditionsOf(decade), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Hip Hop'] } },
    { axis: 'year', not: true, value: { kind: 'range', min: 2000, max: 2009 } },
  ]);
  const butNot = await chipsOfLine('hip hop but not 2000s');
  assert.deepEqual(butNot.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Hip Hop', '2000s!']);
  const two = await chipsOfLine('soul not 1975 or 80s');
  assert.equal(plain(sentenceOf(two)), 'Soul, but not from 1975 or the 1980s');
  const artist = await chipsOfLine('jazz not miles davis');
  assert.deepEqual(artist.map((c) => `${c.reading.kind}:${c.reading.label}/${c.how ?? ''}${c.not ? '!' : ''}`), ['genre:Jazz/', 'artist:Miles Davis/by!']);
  const kept = await chipsOfLine('soul from the 70s but not trap');
  assert.deepEqual(kept.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Soul', '1970s', 'Trap!'], 'a year before "not" stays in');
  // The owner, 2026-10-31: "jazz and blues, but not techno".
  const owner = await chipsOfLine('jazz and blues, but not techno');
  assert.deepEqual(owner.map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Jazz', 'Blues', 'Techno!']);
  assert.equal(plain(sentenceOf(owner, modesFrom(await reads('jazz and blues, but not techno')))), 'Jazz and Blues, but not Techno');
  assert.deepEqual(conditionsOf(owner, CLOSE, { genre: 'and' }), [
    { axis: 'genre', not: false, value: { kind: 'all_of', values: ['Jazz', 'Blues'] } },
    { axis: 'genre', not: true, value: { kind: 'any_of', values: ['Techno'] } },
  ]);
  assert.equal(plain(sentenceOf(await chipsOfLine('jazz and blues not techno'))), 'Jazz or Blues, but not Techno', 'without the comma too');
});

test('"but not" reads back from the conditions and round-trips; "not all of" stays the conditions\' to say', async () => {
  for (const line of ['hip hop but not trap', 'jazz but not on def jam', 'soul but not by al green', 'hip hop from the 90s but not trap or jazz', 'hip hop not 00s', 'soul from the 70s but not 1975 or 80s']) {
    const chips = await chipsOfLine(line);
    const conditions = conditionsOf(chips);
    const back = chipsOf(conditions);
    assert.deepEqual(back.rest, [], line);
    assert.deepEqual(conditionsOf(back.chips, back.min, back.modes), conditions, line);
  }
  const notAll = { axis: 'genre', not: true, value: { kind: 'all_of' as const, values: ['Jazz', 'Soul'] } };
  assert.deepEqual(chipsOf([notAll]).rest, [notAll]);
});

test("a built-in crate's Sounds like said briefly, kept as it is: its own name, else its one seed, else how many", () => {
  const week = { axis: 'like', not: false, value: { kind: 'like' as const, of: ['tr-1', 'tr-2'], count: 50, fresh: true, weights: [1, 0.5] } };
  const r = chipsOf([week], {}, 'Sounds like your week');
  assert.deepEqual(r.chips, []);
  assert.deepEqual(r.rest, []);
  assert.deepEqual(r.brief, { text: 'your week', condition: week });
  assert.equal(plain(sentenceOf([], {}, r.brief!.text)), 'Tracks that sound like your week');
  const air = { axis: 'like', not: false, value: { kind: 'like' as const, of: ['al-1'], count: 40 } };
  assert.equal(chipsOf([air], {}, 'Sounds like Air').brief?.text, 'Air');
  assert.equal(briefOf(['ar-1'], { 'ar-1': { name: 'Ann Quartet', kind: 'artist' } }), 'Ann Quartet');
  const many = { axis: 'like', not: false, value: { kind: 'like' as const, of: ['tr-1', 'tr-2', 'tr-3', 'tr-4', 'tr-5', 'tr-6', 'tr-7'], count: 1000, min: 40 } };
  assert.equal(chipsOf([many]).brief?.text, '7 tracks');
  const jazz = chipsOf([{ axis: 'genre', not: false, value: { kind: 'any_of', values: ['Jazz'] } }, week], {}, 'Sounds like your week');
  assert.equal(plain(sentenceOf(jazz.chips, jazz.modes, jazz.brief!.text)), 'Jazz that sounds like your week');
});

test("the conditions' edits come back to the sentence as typed: a term's words and other readings kept, a new one read plainly", async () => {
  const typed = await chipsOfLine('hip hop from the 90s like miles davis');
  const conditions = conditionsOf(typed);
  const names = namesOf(typed);
  const edited = [...conditions, { axis: 'genre', not: true, value: { kind: 'any_of' as const, values: ['Trap'] } }];
  const back = keptChips(chipsOf(edited, names).chips, typed);
  assert.deepEqual(back.slice(0, typed.length), typed);
  assert.deepEqual(back.slice(typed.length).map((c) => `${c.reading.label}${c.not ? '!' : ''}`), ['Trap!']);
  assert.deepEqual(conditionsOf(back), conditionsOf(chipsOf(edited, names).chips));
});

// One crate card (0.13; the owner, 2026-10-31): global search's and every page's own search, only for words that read as
// a crate of two kinds or more that holds a track, and nothing while its count is not in.
test('the crate card shows only for a crate of two kinds that holds a track, once its count is in', () => {
  const r = (term: string, ...readings: Record<string, unknown>[]) => ({ term, readings });
  const genre = (v: string) => ({ kind: 'genre', value: v, label: v });
  const nineties = { kind: 'year', value: '1990-1999', label: '1990s' };
  const crate = crateOfLine([r('hip hop', genre('Hip Hop')), r('90s', nineties)]);
  assert.equal(crateShows(crate, 12), true);
  assert.equal(crateShows(crate, 0), false, 'an empty crate');
  assert.equal(crateShows(crate, null), false, 'its count not in yet');
  assert.equal(crateShows(crate, undefined), false);
  assert.equal(crateShows(crateOfLine([r('hip hop', genre('Hip Hop'))]), 40), false, 'one kind: no crate');
  assert.equal(crateShows([], 40), false);
});

test("the crate card leads the search of the pages that list music or crates, not Home's or the finders'", () => {
  for (const p of ['library', 'playlists', 'downloads', 'discover', 'requests', 'crates']) assert.equal(crateCardOn(p), true, p);
  for (const p of ['home', 'find', 'settings', 'insights', 'health', 'local-albums']) assert.equal(crateCardOn(p), false, p);
});

test('a line read onto a crate: new terms as chips, a term already in it once, the joins and what read as nothing', () => {
  const r = (term: string, ...readings: Record<string, unknown>[]) => ({ term, readings });
  const genre = (v: string) => ({ kind: 'genre', value: v, label: v });
  const had = crateOfLine([r('jazz', genre('Jazz')), r('1975', { kind: 'year', value: '1975-1975', label: '1975' })])!;
  const out = readOnto(had, [r('Jazz', genre('Jazz')), { ...r('soul', genre('Soul')), join: 'and' }, r('qwzx')]);
  assert.deepEqual(out.add.map((c) => c.reading.label), ['Soul']);
  assert.deepEqual(out.modes, { genre: 'and' }, "an 'and' between two genres joins them");
  assert.deepEqual(out.missed, ['qwzx']);
});

// The tempo (0.14; the owner, 2026-11-02): read as a year is, by rule and only with its "bpm"; never "fast" or "slow".
test('a tempo by rule: a span, open at the top or the bottom, about one; only with its bpm, only a tempo', () => {
  const span = (t: string) => tempoOf(t)?.value ?? null;
  assert.equal(span('120-130 bpm'), '120-130');
  assert.equal(span('120–130 bpm'), '120-130');
  assert.equal(span('120 to 130 bpm'), '120-130');
  assert.equal(span('130 - 120 BPM'), '120-130');
  assert.equal(span('over 140 bpm'), '140-9999');
  assert.equal(span('above 140 bpm'), '140-9999');
  assert.equal(span('under 90 bpm'), '0-90');
  assert.equal(span('below 90 bpm'), '0-90');
  assert.equal(span('128 bpm'), '125-131');
  assert.equal(span('128bpm'), '125-131');
  assert.equal(span('about 128 bpm'), '125-131');
  assert.equal(span('around 128 bpm'), '125-131');
  assert.deepEqual(
    ['120-130 bpm', 'above 140 bpm', 'below 90 bpm', '128 bpm'].map((t) => tempoOf(t)?.label),
    ['120–130 bpm', 'over 140 bpm', 'under 90 bpm', 'about 128 bpm'],
  );
  assert.equal(tempoOf('128 bpm')?.kind, 'bpm');
  for (const t of ['128', '120-130', 'fast', 'slow', '19 bpm', '301 bpm', 'over 400 bpm', '10-130 bpm', 'over 120-130 bpm', 'bpm'])
    assert.equal(tempoOf(t), null, t);
  assert.equal(span('20 to 300 bpm'), '20-300', 'its bounds in');
});

test('a tempo in a line: one term, asking nothing; out of its bounds or without bpm, the words as they read before', async () => {
  const r = await read('hip hop 120-130 bpm, ann quartet');
  assert.deepEqual(r.terms, ['hip hop → genre:Hip Hop', '120-130 bpm → bpm:120–130 bpm', 'ann quartet → artist:Ann Quartet']);
  assert.deepEqual(r.asked, ['hip hop', 'ann quartet'], 'a tempo asks nothing');
  assert.deepEqual((await read('jazz 120 to 130 bpm')).terms, ['jazz → genre:Jazz', '120 to 130 bpm → bpm:120–130 bpm']);
  assert.deepEqual((await read('jazz over 140 bpm')).terms, ['jazz → genre:Jazz', 'over 140 bpm → bpm:over 140 bpm']);
  assert.deepEqual((await read('jazz above 140 bpm')).terms, ['jazz → genre:Jazz', 'above 140 bpm → bpm:over 140 bpm']);
  assert.deepEqual((await read('jazz under 90 bpm')).terms, ['jazz → genre:Jazz', 'under 90 bpm → bpm:under 90 bpm']);
  assert.deepEqual((await read('jazz below 90 bpm')).terms, ['jazz → genre:Jazz', 'below 90 bpm → bpm:under 90 bpm']);
  assert.deepEqual((await read('jazz 128 bpm 90s')).terms, ['jazz → genre:Jazz', '128 bpm → bpm:about 128 bpm', '90s → year:1990s']);
  // Out of a tempo's bounds: no tempo, its words read as any are (here: nothing).
  assert.deepEqual((await read('jazz 400 bpm')).terms, ['jazz → genre:Jazz', '400 → nothing', 'bpm → nothing']);
  // No bpm: a plain number as ever, a year a year.
  assert.deepEqual((await read('jazz 1975')).terms, ['jazz → genre:Jazz', '1975 → year:1975']);
  assert.deepEqual((await read('jazz 1970 to 1979')).terms, ['jazz → genre:Jazz', '1970-1979 → year:1970–1979']);
  assert.deepEqual((await read('jazz 128')).terms, ['jazz → genre:Jazz', '128 → nothing']);
  assert.deepEqual(
    readingsFor('over 140 bpm', [hit('genre', 'Jazz')]).map((x) => `${x.kind}:${x.label}`),
    ['bpm:over 140 bpm', 'genre:Jazz'],
    'a term read again (the term sheet) a tempo first',
  );
});

test('a tempo in the conditions: the bpm range, open at an end; several one span; left out each its own row', async () => {
  const cond = async (line: string) => conditionsOf(await chipsOfLine(line));
  assert.deepEqual(await cond('jazz 120-130 bpm'), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Jazz'] } },
    { axis: 'bpm', not: false, value: { kind: 'range', min: 120, max: 130 } },
  ]);
  assert.deepEqual((await cond('jazz over 140 bpm'))[1], { axis: 'bpm', not: false, value: { kind: 'range', min: 140, max: null } });
  assert.deepEqual((await cond('jazz under 90 bpm'))[1], { axis: 'bpm', not: false, value: { kind: 'range', min: null, max: 90 } });
  assert.deepEqual((await cond('jazz 128 bpm'))[1], { axis: 'bpm', not: false, value: { kind: 'range', min: 125, max: 131 } });
  assert.deepEqual(
    await cond('128 bpm 90s'),
    [
      { axis: 'year', not: false, value: { kind: 'range', min: 1990, max: 1999 } },
      { axis: 'bpm', not: false, value: { kind: 'range', min: 125, max: 131 } },
    ],
    'the years before the tempo, however typed',
  );
  assert.deepEqual(await cond('jazz 100 bpm, over 140 bpm'), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Jazz'] } },
    { axis: 'bpm', not: false, value: { kind: 'range', min: 97, max: null } },
  ]);
  assert.deepEqual(await cond('hip hop not over 140 bpm'), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Hip Hop'] } },
    { axis: 'bpm', not: true, value: { kind: 'range', min: 140, max: null } },
  ]);
});

test('a tempo said as its chip reads, typed back the same, and read back from the conditions as that phrase', async () => {
  const say = async (line: string) => plain(sentenceOf(await chipsOfLine(line)));
  assert.equal(await say('jazz 120-130 bpm'), 'Jazz at 120–130 bpm');
  assert.equal(await say('jazz 120 to 130 bpm'), 'Jazz at 120–130 bpm');
  assert.equal(await say('jazz over 140 bpm'), 'Jazz over 140 bpm');
  assert.equal(await say('jazz above 140 bpm'), 'Jazz over 140 bpm');
  assert.equal(await say('under 90 bpm on blue note'), 'Tracks on Blue Note under 90 bpm');
  assert.equal(await say('jazz from the 90s 128 bpm that sounds like miles davis'), 'Jazz from the 1990s at about 128 bpm that sounds like Miles Davis');
  assert.equal(await say('jazz 100 bpm, over 140 bpm'), 'Jazz at about 100 bpm to over 140 bpm');
  assert.equal(await say('hip hop not over 140 bpm'), 'Hip Hop, but not over 140 bpm');
  assert.equal(await say('soul not 00s or 120-130 bpm'), 'Soul, but not from the 2000s, not at 120–130 bpm');
  for (const line of [
    'jazz 120-130 bpm',
    'jazz over 140 bpm',
    'jazz under 90 bpm',
    'jazz 128 bpm',
    'jazz from the 90s 128 bpm that sounds like miles davis',
    'jazz 100 bpm, over 140 bpm',
    'hip hop not over 140 bpm',
    'soul not 00s or 120-130 bpm',
  ]) {
    const chips = await chipsOfLine(line);
    const sentence = plain(sentenceOf(chips));
    // Typed back: the same terms, the same crate.
    const typed = await chipsOfLine(sentence);
    assert.deepEqual(typed.map((ch) => `${ch.reading.kind}:${ch.reading.label}${ch.not ? '!' : ''}`), chips.map((ch) => `${ch.reading.kind}:${ch.reading.label}${ch.not ? '!' : ''}`), `${sentence}: typed back`);
    // Read back from its conditions: the same crate, said the same where it was one term of a kind.
    const conditions = conditionsOf(chips);
    const back = chipsOf(conditions, namesOf(chips));
    assert.deepEqual(back.rest, [], line);
    assert.deepEqual(conditionsOf(back.chips, back.min, back.modes), conditions, line);
    if (!line.includes(',')) assert.equal(plain(sentenceOf(back.chips)), sentence, `${line}: read back`);
  }
  const back = chipsOf(conditionsOf(await chipsOfLine('jazz 120-130 bpm'))).chips;
  assert.deepEqual(
    back.map((ch) => `${ch.reading.kind}:${ch.reading.label}`),
    ['genre:Jazz', 'bpm:120–130 bpm'],
  );
});

test("a tempo the sentence cannot say as typed stays the conditions': not whole, out of a tempo's bounds, open at both ends", () => {
  const bpm = (min: number | null, max: number | null) => ({ axis: 'bpm', not: false, value: { kind: 'range' as const, min, max } });
  const label = (min: number | null, max: number | null) => chipsOf([bpm(min, max)]).chips[0]?.reading.label;
  assert.equal(label(120, 130), '120–130 bpm');
  assert.equal(label(null, 90), 'under 90 bpm');
  assert.equal(label(140, null), 'over 140 bpm');
  assert.equal(label(125, 131), 'about 128 bpm', 'six wide: about its middle, as a year span of ten is its decade');
  for (const c of [bpm(120.5, 130), bpm(10, 400), bpm(null, 400), bpm(null, null), bpm(15, 21)]) assert.deepEqual(chipsOf([c]).rest, [c], JSON.stringify(c.value));
  assert.equal(tempoSaid(97, null), 'over 97 bpm');
  assert.equal(tempoSaid(17, 23), 'about 20 bpm', 'about a tempo in its bounds');
  assert.equal(tempoSaid(15, 21), null, 'about 18: no tempo');
  const twice = [bpm(120, 130), bpm(90, 100)];
  assert.deepEqual(chipsOf(twice).rest, [twice[1]], 'a second row of one kind');
});
