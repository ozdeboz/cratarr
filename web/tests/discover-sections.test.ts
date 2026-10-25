// Discover's sections (lib/discoverSections, decision 0069 amended 2026-10-25): each suggestion shows once, in the
// first section that takes it; what plays on Bandcamp comes first everywhere; the pick of the day always plays.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { sections, seedsOf, playableFirst } = await import('../src/lib/discoverSections.ts');

const s = (key: string, artist: string, why: string[], more: Partial<{ year: number | null; new_artist: boolean; bandcamp: string; artist_photo: boolean }> = {}) => ({
  key,
  album: key,
  artist,
  year: more.year ?? null,
  new_artist: more.new_artist ?? false,
  why,
  bandcamp: more.bandcamp,
  artist_photo: more.artist_photo,
});

test('the reasons name their seeds, and only those', () => {
  assert.deepEqual(seedsOf('Like Gavin Bryars and Simon Fisher Turner'), ['Gavin Bryars', 'Simon Fisher Turner']);
  assert.deepEqual(seedsOf('Like Air'), ['Air']);
  assert.equal(seedsOf('Like what you play'), null);
  assert.equal(seedsOf('Recommended by ListenBrainz'), null);
});

test('what plays on Bandcamp comes first, the order otherwise kept', () => {
  const list = [s('a', 'x', []), s('b', 'x', [], { bandcamp: 'u' }), s('c', 'x', []), s('d', 'x', [], { bandcamp: 'u' })];
  assert.deepEqual(playableFirst(list).map((x) => x.key), ['b', 'd', 'a', 'c']);
});

test('each suggestion shows once, in the first section that takes it', () => {
  const all = [
    s('rec', 'Röyksopp', ['Recommended by ListenBrainz']),
    s('playable', 'DJ Shadow', ['Recommended by ListenBrainz'], { bandcamp: 'u' }),
    s('gap1', 'Air', ["Missing from Air's albums"], { year: 2004 }),
    s('gap2', 'Air', ["Missing from Air's albums"], { year: 1998, bandcamp: 'u' }),
    s('like1', 'Terry Riley', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('like2', 'Philip Glass', ['Like Simon Fisher Turner and Gavin Bryars'], { new_artist: true }),
    s('like3', 'John Cage', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('like4', 'Like 4', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('like5', 'Like 5', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('like6', 'Like 6', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('like7', 'Like 7', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('like8', 'Like 8', ['Like Gavin Bryars and Simon Fisher Turner'], { new_artist: true }),
    s('new1', 'Foxygen', ['Recommended by ListenBrainz'], { new_artist: true, artist_photo: true }),
    s('new2', 'Unpictured', ['Recommended by ListenBrainz'], { new_artist: true }),
    s('fresh1', 'Air', ['New from Air'], { year: 2026 }),
    s('weekly1', 'Björk', ['In your Weekly Exploration']),
  ];
  const out = sections(all);
  // The pick: the strongest that plays.
  assert.equal(out.pick?.key, 'playable');
  // The gaps: Air's missing albums, what plays first.
  assert.deepEqual(out.gaps.map((x) => x.key), ['gap2', 'gap1']);
  // Around: one group, whichever order its seeds were named in.
  assert.equal(out.around.length, 1);
  assert.deepEqual(out.around[0].items.map((x) => x.key).sort(), ['like1', 'like2', 'like3', 'like4', 'like5', 'like6', 'like7', 'like8']);
  // New to you takes the new artists with a picture the groups did not; one without stays among the cards.
  assert.deepEqual(out.newArtists.map((n) => n.artist), ['Foxygen']);
  assert.deepEqual(out.fresh.map((x) => x.key), ['fresh1']);
  assert.deepEqual(out.weekly.map((x) => x.key), ['weekly1']);
  assert.deepEqual(out.rest.map((x) => x.key), ['rec', 'new2']);
  // Once each: every key in exactly one section.
  const shown = [out.pick!.key, ...out.gaps.map((x) => x.key), ...out.around.flatMap((g) => g.items.map((x) => x.key)), ...out.newArtists.flatMap((n) => n.items.map((x) => x.key)), ...out.fresh.map((x) => x.key), ...out.weekly.map((x) => x.key), ...out.rest.map((x) => x.key)];
  assert.deepEqual([...shown].sort(), all.map((x) => x.key).sort());
});

test('no pick when nothing plays; a group of fewer than three is no section', () => {
  const out = sections([s('a', 'x', ['Like Air']), s('b', 'y', ['Like Air'])]);
  assert.equal(out.pick, null);
  assert.equal(out.around.length, 0);
  assert.deepEqual(out.rest.map((x) => x.key), ['a', 'b']);
});

test('around an artist played: laid out to the room given, what no longer fits waits; too few, no section', () => {
  const like = (k: string, bandcamp?: string) => s(k, k, ['Like Air'], { bandcamp });
  const all = Array.from({ length: 30 }, (_, i) => like(`x${i}`, i % 3 ? undefined : 'u'));
  const out = sections(all, { aroundCols: 3 });
  // The pick takes x0; the grid takes some, the rest wait for the cards.
  assert.equal(out.around.length, 1);
  assert.ok(out.around[0].items.length >= 3 && out.rest.length > 0);
  const wider = sections(all, { aroundCols: 8 });
  assert.ok(wider.around[0].items.length > out.around[0].items.length);
  assert.equal(sections([like('a'), like('b')]).around.length, 0);
});

test('an artist played gathers every suggestion like them, whoever else a reason names (the owner\'s page had none)', () => {
  // As Last.fm's reasons come: each names the seed with a different second artist.
  const all = ['B', 'C', 'D', 'E', 'F', 'G', 'H', 'I'].map((x, i) => s(`k${i}`, `n${i}`, [`Like Holydrug Couple and ${x}`]));
  const out = sections(all, { around: 1, aroundPast: 0 });
  assert.deepEqual(out.around[0].seeds, ['Holydrug Couple']);
  assert.equal(out.around[0].items.length, 8);
});

test('the discography gaps: the artists most played first, a few each, taken in turn', () => {
  const g = (k: string, artist: string) => s(k, artist, [`Missing from ${artist}'s albums`]);
  // In the list's order (the server's: the most played first); A has the most missing, which no longer puts it first.
  const all = [g('b1', 'B'), g('a1', 'A'), g('a2', 'A'), g('a3', 'A'), g('a4', 'A'), g('a5', 'A'), g('b2', 'B'), g('c1', 'C')];
  const out = sections(all, { gaps: 2 });
  assert.deepEqual(out.gaps.map((x) => x.key), ['b1', 'a1', 'b2', 'a2', 'a3', 'a4']);
});

test("new from artists you have: ListenBrainz's fresh releases and the discographies' of this year and last", () => {
  const g = (k: string, year: number) => s(k, 'A', ["Missing from A's albums"], { year });
  const all = [g('old', 2009), g('last', 2025), g('now', 2026), s('lb', 'B', ['New from B'], { year: 2026 })];
  const out = sections(all, { thisYear: 2026 });
  assert.deepEqual(out.fresh.map((x) => x.key).sort(), ['last', 'lb', 'now']);
  assert.deepEqual(out.gaps.map((x) => x.key), ['old']);
});
