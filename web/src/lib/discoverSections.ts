// Discover as a page of sections (decision 0069, amended 2026-10-25: "a more dynamic page", each section the form its
// content suits): the suggestions, already filtered and searched, sorted into the pick of the day, the discography
// gaps of the artists most played, the groups around the artists played, the artists new to it, the fresh releases and the
// Weekly Exploration; what none of those takes stays as the cards. Each suggestion shows once, in the first section
// that takes it, and within each what plays on Bandcamp comes first (the owner, 2026-10-25). Pure: what the page draws.

/** As much of a suggestion as the sections read (api.ts's DiscoverAlbum). */
export type Suggested = {
  key: string;
  album: string;
  artist: string;
  year: number | null;
  new_artist: boolean;
  why: string[];
  bandcamp?: string;
};

export type Around<S extends Suggested = Suggested> = { seeds: string[]; items: S[]; wide: string[] };
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
export function sections<S extends Suggested>(all: S[], limits = { gaps: 4, gapsEach: 4, around: 1, aroundCells: 8, newArtists: 10 }): Sections<S> {
  const used = new Set<string>();
  const take = (s: S) => used.add(s.key);
  const free = (s: S) => !used.has(s.key);

  // The pick of the day: the strongest that plays on Bandcamp, else none.
  const pick = all.find((s) => s.bandcamp) ?? null;
  if (pick) take(pick);

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
  // The eight places around the seeds, always all filled (the owner, 2026-10-25): what plays takes two (a tile of one
  // by two), the rest one, so one tile of two makes seven suggestions; a group that cannot fill them all is no
  // section, and what no longer fits waits for the cards.
  const fill = (g: { seeds: string[]; items: S[] }): Around<S> | null => {
    let cells = limits.aroundCells;
    const items: S[] = [];
    const wide: string[] = [];
    const ready = playableFirst(g.items).filter(free);
    for (const s of ready) {
      if (!cells) break;
      items.push(s);
      if (s.bandcamp) wide.push(s.key);
      cells -= s.bandcamp ? 2 : 1;
    }
    return cells ? null : { seeds: g.seeds, items, wide };
  };
  const around: Around<S>[] = [];
  for (const g of [...groups.values()].sort((a, b) => cellsOf(b) - cellsOf(a) || b.items.length - a.items.length)) {
    if (around.length >= limits.around) break;
    const f = fill(g);
    if (!f) continue;
    f.items.forEach(take);
    around.push(f);
  }

  // New to you: per new artist its suggestions, the first that plays first.
  const newBy = new Map<string, S[]>();
  for (const s of playableFirst(all)) if (free(s) && s.new_artist) newBy.set(s.artist, [...(newBy.get(s.artist) ?? []), s]);
  const newArtists = [...newBy.entries()].slice(0, limits.newArtists).map(([artist, items]) => {
    items.forEach(take);
    return { artist, items };
  });

  // The fresh releases, newest first; the Weekly Exploration as it came.
  const fresh = all
    .filter((s) => free(s) && s.why.some((w) => FRESH.test(w)))
    .sort((a, b) => Number(!!b.bandcamp) - Number(!!a.bandcamp) || (b.year ?? 0) - (a.year ?? 0));
  fresh.forEach(take);
  const weekly = playableFirst(all.filter((s) => free(s) && s.why.includes(WEEKLY)));
  weekly.forEach(take);

  return { pick, gaps, around, newArtists, fresh, weekly, rest: playableFirst(all.filter(free)) };
}
