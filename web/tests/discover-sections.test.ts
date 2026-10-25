// Discover's sections (lib/discoverSections, decision 0069 amended 2026-10-25): each suggestion shows once, in the
// first section that takes it; what plays on Bandcamp comes first everywhere; the pick of the day always plays.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { sections, seedsOf, playableFirst } = await import('../src/lib/discoverSections.ts');

const s = (key: string, artist: string, why: string[], more: Partial<{ year: number | null; new_artist: boolean; bandcamp: string }> = {}) => ({
  key,
  album: key,
  artist,
  year: more.year ?? null,
  new_artist: more.new_artist ?? false,
  why,
  bandcamp: more.bandcamp,
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
    s('new1', 'Foxygen', ['Recommended by ListenBrainz'], { new_artist: true }),
    s('fresh1', 'Air', ['New from Air'], { year: 2026 }),
    s('weekly1', 'Björk', ['In your Weekly Exploration']),
  ];
  const owned = [{ title: 'Moon Safari', artist: 'Air', year: 1998 }, { title: '10 000 Hz Legend', artist: 'Air', year: 2001 }];
  const out = sections(all, owned);
  // The pick: the strongest that plays.
  assert.equal(out.pick?.key, 'playable');
  // The gaps: Air's line in time, the library's albums among the missing ones.
  assert.equal(out.gaps.length, 1);
  assert.deepEqual(
    out.gaps[0].points.map((p) => (p.kind === 'have' ? `have ${p.title}` : `gap ${p.s.key}`)),
    ['have Moon Safari', 'gap gap2', 'have 10 000 Hz Legend', 'gap gap1'],
  );
  // Around: one group, whichever order its seeds were named in.
  assert.equal(out.around.length, 1);
  assert.deepEqual(out.around[0].items.map((x) => x.key).sort(), ['like1', 'like2', 'like3', 'like4', 'like5', 'like6', 'like7', 'like8']);
  // New to you takes the new artists the groups did not.
  assert.deepEqual(out.newArtists.map((n) => n.artist), ['Foxygen']);
  assert.deepEqual(out.fresh.map((x) => x.key), ['fresh1']);
  assert.deepEqual(out.weekly.map((x) => x.key), ['weekly1']);
  assert.deepEqual(out.rest.map((x) => x.key), ['rec']);
  // Once each: every key in exactly one section.
  const shown = [out.pick!.key, ...out.gaps.flatMap((g) => g.points.flatMap((p) => (p.kind === 'gap' ? [p.s.key] : []))), ...out.around.flatMap((g) => g.items.map((x) => x.key)), ...out.newArtists.flatMap((n) => n.items.map((x) => x.key)), ...out.fresh.map((x) => x.key), ...out.weekly.map((x) => x.key), ...out.rest.map((x) => x.key)];
  assert.deepEqual([...shown].sort(), all.map((x) => x.key).sort());
});

test('no pick when nothing plays; a group of fewer than three is no section', () => {
  const out = sections([s('a', 'x', ['Like Air']), s('b', 'y', ['Like Air'])], []);
  assert.equal(out.pick, null);
  assert.equal(out.around.length, 0);
  assert.deepEqual(out.rest.map((x) => x.key), ['a', 'b']);
});

test('around the seeds all eight places are filled: what plays takes two, what no longer fits waits', () => {
  const like = (k: string, bandcamp?: string) => s(k, k, ['Like Air'], { bandcamp });
  const limits = { gaps: 4, around: 1, aroundCells: 8, newArtists: 10 };
  // The pick takes p1; p2, p3 and p4 take six places, a and b the last two; c is left for the cards.
  let out = sections([like('p1', 'u'), like('p2', 'u'), like('p3', 'u'), like('a'), like('b'), like('c'), like('p4', 'u')], [], limits);
  assert.deepEqual(out.around[0].items.map((x) => x.key), ['p2', 'p3', 'p4', 'a', 'b']);
  assert.deepEqual(out.around[0].wide, ['p2', 'p3', 'p4']);
  assert.deepEqual(out.rest.map((x) => x.key), ['c']);
  // Four that play fill the eight on their own.
  out = sections([like('p1', 'u'), like('p2', 'u'), like('a'), like('p3', 'u'), like('p4', 'u'), like('p5', 'u')], [], limits);
  assert.deepEqual(out.around[0].wide, ['p2', 'p3', 'p4', 'p5']);
  assert.deepEqual(out.rest.map((x) => x.key), ['a']);
  // Not enough to fill the eight: no section.
  out = sections([like('p1', 'u'), like('p2', 'u'), like('a'), like('b'), like('c')], [], limits);
  assert.equal(out.around.length, 0);
});

test('an artist played gathers every suggestion like them, whoever else a reason names (the owner\'s page had none)', () => {
  // As Last.fm's reasons come: each names the seed with a different second artist.
  const all = ['B', 'C', 'D', 'E', 'F', 'G', 'H', 'I'].map((x, i) => s(`k${i}`, `n${i}`, [`Like Holydrug Couple and ${x}`]));
  const out = sections(all, [], { gaps: 4, around: 1, aroundCells: 8, newArtists: 10 });
  assert.deepEqual(out.around[0].seeds, ['Holydrug Couple']);
  assert.equal(out.around[0].items.length, 8);
});
