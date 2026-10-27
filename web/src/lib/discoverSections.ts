// Discover as a page of sections (decision 0069, amended 2026-10-25: "a more dynamic page", each section the form its
// content suits): the suggestions, already filtered and searched, sorted into the pick of the day, the discography
// gaps of the artists most played, the groups around the artists played, the artists new to it, the fresh releases and the
// Weekly Exploration; what none of those takes stays as the cards. Each suggestion shows once, in the first section
// that takes it, and within each what plays on Bandcamp comes first (the owner, 2026-10-25). Pure: what the page draws.

import { layout, type Grid } from './masonry';

/** As much of a suggestion as the sections read (api.ts's DiscoverAlbum). */
export type Suggested = {
  key: string;
  album: string;
  artist: string;
  year: number | null;
  new_artist: boolean;
  why: string[];
  bandcamp?: string;
  /** Its artist, new to the library, has a picture (New to you shows only those). */
  artist_photo?: boolean;
  /** Its new artist has anything on Bandcamp (New to you puts those first). */
  artist_on_bandcamp?: boolean;
  /** How alike it is to each artist played it was found from (Last.fm's match, 0 to 1), where known. */
  alike?: { seed: string; value: number }[];
};

/** A group around its artist (and the library's artist closest to it, `partner`), laid out (lib/masonry). */
export type Around<S extends Suggested = Suggested> = { seeds: string[]; partner: string | null; items: S[]; grid: Grid };
export type Sections<S extends Suggested> = {
  pick: S | null;
  gaps: S[];
  around: Around<S>[];
  newArtists: { artist: string; items: S[] }[];
  fresh: S[];
  weekly: S[];
  rest: S[];
};

const GAP = /^Missing from (.+)'s albums$/;
const FRESH = /^New from /;
const LIKE = /^Like (.+)$/;
const WEEKLY = 'In your Weekly Exploration';

/** What plays on Bandcamp first, the order otherwise kept. */
export const playableFirst = <S extends Suggested>(list: S[]): S[] => [...list.filter((s) => s.bandcamp), ...list.filter((s) => !s.bandcamp)];

/** A name as names are compared here (case, spaces and punctuation aside). */
const key = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');

/** The seeds of a "Like A and B" reason, else null. */
export function seedsOf(why: string): string[] | null {
  const m = LIKE.exec(why);
  if (!m || m[1] === 'what you play') return null;
  return m[1].split(' and ').map((x) => x.trim()).filter(Boolean);
}

/** The sections, from the suggestions in their order (the strongest first). */
/** How much each section takes; `aroundCols`: how many masonry columns the page has room for (lib/masonry);
 *  `aroundPast`: how many more each group's grid takes, so it runs past the page's edge (the owner, 2026-10-26: more
 *  than the width, scrolled a little, never empty room); `aroundRound`: Around's refreshes, each on to the next of each
 *  artist's suggestions. */
export const LIMITS = { gaps: 4, gapsEach: 4, around: 3, aroundCols: 3, aroundPast: 2, aroundRound: 0, newArtists: 10, thisYear: new Date().getFullYear() };
/** How far a refresh moves on in each artist's suggestions: about a grid's worth. */
const ROUND_STEP = 12;
/** The list from its `n`th on, the ones before after it (a refresh: the others first, none lost). */
const turned = <T>(list: T[], n: number) => (list.length ? [...list.slice(n % list.length), ...list.slice(0, n % list.length)] : list);

/** `notPick`: what is no pick of the day (moved on from in Discover's menu, kept for the account; or asked for already):
 *  the next that plays is the pick. `aroundFrom`: the server's own Around (0.9.7): its artists in order and their
 *  related artists' albums; null for a server before it (the groups then come from `all`, as before). */
export function sections<S extends Suggested>(
  all: S[],
  given: Partial<typeof LIMITS> = {},
  notPick: (s: S) => boolean = () => false,
  aroundFrom: { seeds: string[]; albums: S[] } | null = null,
): Sections<S> {
  const limits = { ...LIMITS, ...given };
  const used = new Set<string>();
  const take = (s: S) => used.add(s.key);
  const free = (s: S) => !used.has(s.key);

  // The pick of the day: the strongest that plays on Bandcamp and is no pick's to pass (moved on from, or asked for
  // already: the owner, 2026-10-26, a monitored album came up), else none.
  const pick = all.find((s) => s.bandcamp && !notPick(s)) ?? null;
  if (pick) take(pick);

  // New from artists you have: ListenBrainz's fresh releases, and the discographies' releases of this year and last
  // (the owner, 2026-10-25: the row had 3, the discography gaps held 22 from 2025 and 2026); what plays first, then
  // newest first. Taken before the gaps, which keep the older ones.
  const recent = (s: S) => s.year !== null && s.year >= limits.thisYear - 1 && s.why.some((w) => GAP.test(w));
  const fresh = all
    .filter((s) => free(s) && (s.why.some((w) => FRESH.test(w)) || recent(s)))
    .sort((a, b) => Number(!!b.bandcamp) - Number(!!a.bandcamp) || (b.year ?? 0) - (a.year ?? 0));
  fresh.forEach(take);

  // Discography gaps (the owner, 2026-10-25: one shelf, as Home's New for you): the albums missing from the artists
  // most played (the list's order, the server's: "relevant for the most played artists"), a few each and taken in
  // turn, so one prolific artist does not fill the row; within an artist what plays first.
  const byArtist = new Map<string, S[]>();
  for (const s of all) if (free(s) && s.why.some((w) => GAP.test(w))) byArtist.set(s.artist, [...(byArtist.get(s.artist) ?? []), s]);
  const lines = [...byArtist.values()].slice(0, limits.gaps).map((l) => playableFirst(l).slice(0, limits.gapsEach));
  const gaps: S[] = [];
  for (let i = 0; lines.some((l) => l[i]); i++) for (const l of lines) if (l[i]) gaps.push(l[i]);
  gaps.forEach(take);

  // Around the artists played: per artist played, the suggestions like them ("Like A and B" counts for A and for B:
  // grouping by the pair split them into groups too small to fill the eight places, the owner's 2026-10-25 page had
  // none); the groups that fill the most places first.
  const groups = new Map<string, { seeds: string[]; items: S[] }>();
  for (const s of all) {
    if (!free(s)) continue;
    const seeds = s.why.map(seedsOf).find((x) => x) ?? [];
    for (const seed of seeds) {
      const g = groups.get(key(seed)) ?? { seeds: [seed], items: [] };
      g.items.push(s);
      groups.set(key(seed), g);
    }
  }
  const cellsOf = (g: { items: S[] }) => g.items.reduce((n, s) => n + (s.bandcamp ? 2 : 1), 0);
  // Laid out as wide as the page allows (lib/masonry, the owner, 2026-10-25, 0.9.2): the first artist's grid as large
  // as its suggestions fill, of those still close to it (at least half as alike as the most alike, where Last.fm said
  // how alike); the width left goes to the next artist, by the same rule.
  const close = (seed: string, items: S[]) => {
    const of = (s: S) => s.alike?.find((x) => key(x.seed) === key(seed))?.value;
    const top = Math.max(0, ...items.map((s) => of(s) ?? 0));
    return items.filter((s) => of(s) === undefined || !top || of(s)! >= top / 2);
  };
  const around: Around<S>[] = [];
  // Each group its own row, as wide as the page and a little more (it scrolls); no longer side by side.
  const width = limits.aroundCols + limits.aroundPast;
  // The server's artists, in its order, a group each: their related artists' albums, then Last.fm's suggestions like
  // them from the list (the owner, 2026-10-26: those stay in Around, after the artists' own).
  for (const seed of aroundFrom?.seeds ?? []) {
    const of = (s: S) => s.why.some((w) => seedsOf(w)?.some((x) => key(x) === key(seed)));
    const own = aroundFrom!.albums.filter((s) => free(s) && of(s));
    const lastfm = all.filter((s) => free(s) && of(s) && !own.some((o) => o.key === s.key));
    const ready = [...close(seed, playableFirst(own)), ...close(seed, playableFirst(lastfm))];
    const laid = layout(
      ready.map((s) => ({ key: s.key, plays: !!s.bandcamp, s })),
      width,
      seed,
    );
    if (!laid) continue;
    const items = laid.items.map((x) => x.s);
    items.forEach(take);
    around.push({ seeds: [seed], partner: null, items, grid: laid.grid });
  }
  const ranked = aroundFrom ? [] : [...groups.values()].sort((a, b) => cellsOf(b) - cellsOf(a) || b.items.length - a.items.length);
  const partnered = new Set<string>();
  for (const g of ranked) {
    if (around.length >= limits.around) break;
    if (partnered.has(key(g.seeds[0]))) continue;
    const mine = turned(close(g.seeds[0], playableFirst(g.items).filter(free)), limits.aroundRound * ROUND_STEP);
    // The library's artist closest to this one (the owner, 2026-10-25: "use that in the row as well"): one Last.fm
    // found suggestions for together with it ("Like A and B"), of those the one adding the most of its own (the one
    // sharing the most added none: the same suggestions, a place less); theirs join, after this artist's own.
    const shared = (h: { items: S[] }) => h.items.filter((s) => g.items.includes(s)).length;
    const adds = (h: { items: S[] }) => h.items.filter((s) => free(s) && !g.items.includes(s)).length;
    const other = ranked
      .filter((h) => h !== g && !partnered.has(key(h.seeds[0])) && shared(h) > 0 && adds(h) > 0)
      .sort((a, b) => adds(b) - adds(a) || shared(b) - shared(a))[0];
    const theirs = other ? close(other.seeds[0], playableFirst(other.items).filter((s) => free(s) && !mine.includes(s))) : [];
    const ready = [...mine, ...theirs];
    const placeable = ready.map((s) => ({ key: s.key, plays: !!s.bandcamp, s }));
    const laid = (other && layout(placeable, width, g.seeds[0], true)) || layout(placeable.filter((x) => mine.includes(x.s)), width, g.seeds[0]);
    if (!laid) continue;
    const items = laid.items.map((x) => x.s);
    items.forEach(take);
    const partner = laid.grid.partner && other ? other.seeds[0] : null;
    if (partner) partnered.add(key(partner));
    around.push({ seeds: g.seeds, partner, items, grid: laid.grid });
  }

  // New to you: per new artist with a picture (a flow of their pictures, the owner 2026-10-25) its suggestions, the
  // first that plays first; those without one stay among the cards.
  const newBy = new Map<string, S[]>();
  for (const s of playableFirst(all)) if (free(s) && s.new_artist && s.artist_photo) newBy.set(s.artist, [...(newBy.get(s.artist) ?? []), s]);
  // Those with anything on Bandcamp first, to Listen to (the owner, 2026-10-27: the others deprioritised, not dropped).
  const onBandcamp = (items: S[]) => items.some((s) => s.artist_on_bandcamp || s.bandcamp);
  const newArtists = [...newBy.entries()]
    .sort((a, b) => Number(onBandcamp(b[1])) - Number(onBandcamp(a[1])))
    .slice(0, limits.newArtists)
    .map(([artist, items]) => {
      items.forEach(take);
      return { artist, items };
    });

  // The Weekly Exploration as it came.
  const weekly = playableFirst(all.filter((s) => free(s) && s.why.includes(WEEKLY)));
  weekly.forEach(take);

  return { pick, gaps, around, newArtists, fresh, weekly, rest: playableFirst(all.filter(free)) };
}
