// Quick crates (0.11; the owner, 2026-10-29 and 2026-10-30): a crate made from a few typed terms, each a chip. A
// genre, style, mood, tag, label, album or year (or decade) narrows it; an artist means "sounds like" them, the chip
// switching to "by" them, their own tracks left out unless kept. Filtered first, then ranked by how alike, cut where
// it stops being close (the closeness curve's handle loosens it). A word that reads several ways takes its best
// reading, its kind shown, the others a tap away. Pure: terms in, chips and a crate's conditions out.
import type { Condition } from './api';
import { byRelevance, relevance, words, type SearchResult } from './search';

export type ChipKind = 'artist' | 'album' | 'track' | 'label' | 'genre' | 'style' | 'mood' | 'tag' | 'year';
/** One way a term reads: its kind, what a condition matches (a name, an album's id, a year's span `min-max`), and how
 *  it shows. An artist's `id` (the library's `ar-…`) seeds "sounds like". */
export type Reading = { kind: ChipKind; value: string; label: string; id?: string };
/** A typed term made a chip: its reading, the other ways it could read, and for an artist "like" or "by" them and
 *  whether their own tracks stay. */
export type Chip = { term: string; reading: Reading; others: Reading[]; how?: 'like' | 'by'; own?: boolean };

/** Where the cut starts: a close fit (0–100, the closeness curve's scale; the owner: "Close, adjustable"). Provisional
 *  until measured on a library. */
export const CLOSE = 60;
/** How many the closest may be at most, the cut deciding below it (the server's own limit). */
export const LIKE_MOST = 1000;

/** The kinds search is asked for, a term's readings among them. */
export const TERM_KINDS = ['artist', 'album', 'label', 'genre', 'style', 'mood', 'tag'] as const;
/** Equally good readings in this order: what narrows broadly before a name (the owner's example: "Soul" a genre
 *  before an artist of that name). */
const ORDER: ChipKind[] = ['year', 'genre', 'style', 'mood', 'artist', 'label', 'album', 'tag'];

/** The terms typed: apart at commas, semicolons and line breaks, each trimmed, a term typed twice once. */
export function splitTerms(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const t of text.split(/[,;\n]+/)) {
    const term = t.trim().replace(/\s+/g, ' ');
    const key = term.toLowerCase();
    if (!term || seen.has(key)) continue;
    seen.add(key);
    out.push(term);
  }
  return out;
}

/** What search found for a phrase: its hits of the term kinds (a few of each), as a phrase's readings come from. */
export type Found = readonly (SearchResult & { id?: string })[];
/** Asks search for a phrase (the sheet's: the term kinds, a few of each). */
export type Ask = (phrase: string) => Promise<Found>;
/** A phrase read off a line: the words it took, and their readings, the best first (none: nothing found). */
export type Read = { term: string; readings: Reading[] };

/** The longest phrase tried past a line's whole run (the owner, 2026-10-30: "Read the whole line"): names of up to
 *  three words read together, a longer one where the whole run is it ("the dark side of the moon"). */
export const MOST_WORDS = 3;
/** A phrase fits a hit when the hit's own name holds its words and they make up this much of the name (a few letters
 *  still to type, or "hiphop" for Hip Hop): "miles davis" Miles Davis, not "miles" Miles Davis Quintet. */
export const FIT = 0.7;

/** Whether one of a phrase's hits is the phrase: its own name holds the words, the words most of the name. A guess
 *  (the matcher's fuzzy: typos forgiven) fits only a phrase about as long as the name ("miles daivs" Miles Davis), so
 *  a word too many is read apart, not swallowed ("jazz funk soul" is not Jazz-Funk). */
export function fits(phrase: string, found: Found): boolean {
  const q = words(phrase);
  const typed = q.join('').length;
  return found.some((r) => {
    if (!readingOf(r)) return false;
    const rel = relevance(q, r);
    const guess = rel[1] < 0;
    return rel[0] === 1 && rel[5] >= FIT && (!guess || typed <= words(r.title).join('').length / FIT);
  });
}

/** A line read as the owner types it (2026-10-30: "hip hop 90s miles davis"): apart at commas and semicolons
 *  (`splitTerms`); in each piece the years and decades first, by rule; the words between them read longest first,
 *  the whole run, then three words, two, one, a phrase taken where one of its hits fits it (`fits`); where none fits,
 *  the one word, read by its best hit as a term always was. Each phrase asked once; the shorter ones at a place
 *  together, after the longest did not fit. In the order typed. */
export async function readLine(text: string, ask: Ask, now?: number): Promise<Read[]> {
  const asked = new Map<string, Promise<Found>>();
  const once = (phrase: string) => {
    const key = words(phrase).join(' ');
    let a = asked.get(key);
    if (!a) asked.set(key, (a = ask(phrase).catch(() => [] as Found)));
    return a;
  };
  const read = (term: string, found: Found): Read => ({ term, readings: readingsFor(term, found, now) });

  async function segment(tokens: string[]): Promise<Read[]> {
    const out: Read[] = [];
    for (let i = 0; i < tokens.length; ) {
      const phrase = (n: number) => tokens.slice(i, i + n).join(' ');
      const longest = i === 0 ? tokens.length : Math.min(MOST_WORDS, tokens.length - i);
      const first = await once(phrase(longest));
      let took = longest;
      let found = first;
      if (!fits(phrase(longest), first) && longest > 1) {
        const shorter = Array.from({ length: Math.min(MOST_WORDS, longest - 1) }, (_, k) => Math.min(MOST_WORDS, longest - 1) - k);
        const tries = await Promise.all(shorter.map(async (n) => ({ n, found: await once(phrase(n)) })));
        const fit = tries.find((x) => fits(phrase(x.n), x.found)) ?? tries[tries.length - 1];
        took = fit.n;
        found = fit.found;
      }
      out.push(read(phrase(took), found));
      i += took;
    }
    return out;
  }

  const pieces = await Promise.all(
    splitTerms(text).map(async (piece) => {
      const tokens = piece.replace(/(\d{4})\s*[-–]\s*(\d{4})/g, '$1-$2').split(' ');
      const parts: (Read | string[])[] = [];
      for (const t of tokens) {
        if (!words(t).length) continue;
        const year = yearOf(t, now);
        if (year) parts.push({ term: t, readings: [year] });
        else if (Array.isArray(parts.at(-1))) (parts.at(-1) as string[]).push(t);
        else parts.push([t]);
      }
      const reads = await Promise.all(parts.map((p) => (Array.isArray(p) ? segment(p) : Promise.resolve([p]))));
      return reads.flat();
    }),
  );
  const seen = new Set<string>();
  return pieces.flat().filter((r) => {
    const key = r.term.toLowerCase();
    return !seen.has(key) && (seen.add(key), true);
  });
}

/** A year or a decade as typed: 1975, '75, 1970s, '70s, 70s, 1970-1979; `now` for which century a two-digit year is
 *  (one after this year's two digits is last century's). */
export function yearOf(term: string, now = new Date().getFullYear()): Reading | null {
  const t = term.trim().toLowerCase().replace(/^[’‘]/, "'");
  const two = (yy: number) => (yy <= now % 100 ? 2000 + yy : 1900 + yy);
  const span = (min: number, max: number, label: string): Reading => ({ kind: 'year', value: `${min}-${max}`, label });
  let m = /^(\d{4})\s*[-–]\s*(\d{4})$/.exec(t);
  if (m) {
    const [a, b] = [Number(m[1]), Number(m[2])];
    return span(Math.min(a, b), Math.max(a, b), `${Math.min(a, b)}–${Math.max(a, b)}`);
  }
  m = /^(\d{3})0'?s$/.exec(t);
  if (m) return span(Number(m[1]) * 10, Number(m[1]) * 10 + 9, `${m[1]}0s`);
  m = /^'?(\d)0'?s$/.exec(t);
  if (m) {
    const d = two(Number(m[1]) * 10);
    return span(d, d + 9, `${d}s`);
  }
  m = /^(\d{4})$/.exec(t);
  if (m && Number(m[1]) >= 1000) return span(Number(m[1]), Number(m[1]), m[1]);
  m = /^'(\d{2})$/.exec(t);
  if (m) {
    const y = two(Number(m[1]));
    return span(y, y, String(y));
  }
  return null;
}

/** A search result as a reading, where it is one (a track, a playlist or a crate is not). */
export function readingOf(r: SearchResult & { id?: string }): Reading | null {
  const k = r.kind;
  if (k === 'artist') return { kind: 'artist', value: r.title, label: r.title, ...(r.id ? { id: r.id } : {}) };
  if (k === 'album') {
    const id = /^album:[^/]+\/(.+)$/.exec(r.subject)?.[1];
    return id ? { kind: 'album', value: id, label: r.title } : null;
  }
  if (k === 'label' || k === 'genre' || k === 'style' || k === 'mood' || k === 'tag') return { kind: k, value: r.title, label: r.title };
  return null;
}

/** A term's readings, the best first: a year or decade where it is one, then search's finds by how well they match the
 *  term (its words, the name's own first), equals in ORDER; each once. */
export function readingsFor(term: string, found: readonly (SearchResult & { id?: string })[], now?: number): Reading[] {
  const q = words(term);
  const year = yearOf(term, now);
  const ranked = found
    .map((r) => ({ read: readingOf(r), rel: relevance(q, r) }))
    .filter((x): x is { read: Reading; rel: number[] } => x.read !== null)
    .sort((a, b) => byRelevance(a.rel, b.rel) || ORDER.indexOf(a.read.kind) - ORDER.indexOf(b.read.kind));
  const out: Reading[] = year ? [year] : [];
  for (const { read } of ranked) if (!out.some((o) => o.kind === read.kind && o.value === read.value)) out.push(read);
  return out;
}

/** A chip from a term's readings (none: no chip); an artist "sounds like" by default where it can seed (its id). */
export function chipOf(term: string, readings: readonly Reading[]): Chip | null {
  const [reading, ...others] = readings;
  if (!reading) return null;
  return { term, reading, others, ...(reading.kind === 'artist' ? { how: reading.id ? ('like' as const) : ('by' as const), own: false } : {}) };
}

/** The chip read another way (a tap on another reading): the one it read becomes one of the others. */
export function readAs(chip: Chip, reading: Reading): Chip {
  const others = [chip.reading, ...chip.others].filter((r) => !(r.kind === reading.kind && r.value === reading.value));
  const artist = reading.kind === 'artist' ? { how: reading.id ? (chip.how ?? 'like') : ('by' as const), own: chip.own ?? false } : {};
  return { term: chip.term, reading, others, ...artist };
}

/** A chip that seeds "sounds like": an artist read that way, or a track or an album a menu's "Sounds like this" handed
 *  over (0.12; the owner: one way to make a crate), each by its library id. */
const likes = (c: Chip) => c.how === 'like' && !!c.reading.id;

/** "Sounds like this" from a menu (0.12): the track, album or artist as the crate's "like" term, by its library id
 *  (`tr-…`, `al-…`, `ar-…`); their own tracks left out, as a typed artist's are. */
export function likeChip(kind: 'track' | 'album' | 'artist', id: string, label: string): Chip {
  return { term: label, reading: { kind, value: kind === 'artist' ? label : id, label, id }, others: [], how: 'like', own: false };
}

/** "Grab a crate" from a genre's, a style's, a mood's, a tag's or a label's own crate (0.12): that word as its term. */
export function wordChip(kind: 'genre' | 'style' | 'mood' | 'tag' | 'label', word: string): Chip {
  return { term: word, reading: { kind, value: word, label: word }, others: [] };
}

/** Global search's words read as a crate (0.12; the owner, 2026-10-30: mockup 1a, the crate card first): its chips
 *  where they read as two terms or more of two kinds or more (a year and an artist; a genre, a decade and an artist),
 *  null where they are one thing (a name, a genre), which the plain Grab a crate row stays for. */
export function crateOfLine(reads: readonly Read[]): Chip[] | null {
  const chips = reads.flatMap((r) => chipOf(r.term, r.readings) ?? []);
  const kinds = new Set(chips.map((c) => c.reading.kind));
  return chips.length >= 2 && kinds.size >= 2 ? chips : null;
}

/** The seeds of "sounds like": the artists chipped that way. */
export const seedsOf = (chips: readonly Chip[]): string[] => chips.filter(likes).map((c) => c.reading.id!);

/** Whether the artists chipped "like" keep their own tracks (one of them keeping them keeps them). */
export const keepsOwn = (chips: readonly Chip[]) => chips.some((c) => likes(c) && !!c.own);

/** The chips as a crate's conditions: each kind's chips one row, any of them (two genres: either); the years one span
 *  from the earliest to the latest; the artists "by" them one row; those "like" them one Sounds like, cut at `min`,
 *  their own tracks out unless one of them keeps them. */
export function conditionsOf(chips: readonly Chip[], min = CLOSE): Condition[] {
  const rows: Condition[] = [];
  const byAxis = new Map<string, string[]>();
  const add = (axis: string, v: string) => {
    const vs = byAxis.get(axis) ?? [];
    if (!vs.includes(v)) vs.push(v);
    byAxis.set(axis, vs);
  };
  let years: [number, number] | null = null;
  for (const c of chips) {
    const r = c.reading;
    if (likes(c) || r.kind === 'track') continue;
    if (r.kind === 'year') {
      const [a, b] = r.value.split('-').map(Number);
      years = years ? [Math.min(years[0], a), Math.max(years[1], b)] : [a, b];
    } else add(r.kind, r.value);
  }
  for (const [axis, values] of byAxis) rows.push({ axis, not: false, value: { kind: 'any_of', values } });
  if (years) rows.push({ axis: 'year', not: false, value: { kind: 'range', min: years[0], max: years[1] } });
  const seeds = seedsOf(chips);
  if (seeds.length) {
    rows.push({ axis: 'like', not: false, value: { kind: 'like', of: seeds, count: LIKE_MOST, min, ...(keepsOwn(chips) ? { own: true } : {}) } });
  }
  return rows;
}

/** A part of a crate's sentence: a term (its chip's place, a tap reads it another way) or the words between; `tight`:
 *  no space before it (a comma). */
export type Part = { text: string; chip?: number; tight?: boolean };

/** The crate read as a sentence (0.12; the owner, 2026-10-30: the crate search's mockup B): "Hip Hop from the 1990s that
 *  sounds like Miles Davis". As `conditionsOf` combines them, so it says what the crate is: a kind's terms any of them
 *  ("or"), the kinds all of them; the years one span from the earliest to the latest; the artists "like" one Sounds
 *  like of them all ("and", as the server names it). Genres lead (else styles, else "Tracks"); then a style beside a
 *  genre, tags, by, on a label, from an album, the years; last what it feels and sounds like. */
export function sentenceOf(chips: readonly Chip[]): Part[] {
  const all = chips.map((c, i) => ({ c, i }));
  const of = (kind: ChipKind) => all.filter(({ c }) => c.reading.kind === kind && !likes(c));
  const out: Part[] = [];
  const word = (text: string, tight = false) => out.push(tight ? { text, tight } : { text });
  const term = (i: number, text = chips[i].reading.label) => out.push({ text, chip: i });
  const list = (items: { i: number }[], join: 'or' | 'and' = 'or') =>
    items.forEach(({ i }, k) => {
      if (k) k === items.length - 1 ? word(join) : word(',', true);
      term(i);
    });

  const genres = of('genre');
  const styles = of('style');
  const head = genres.length ? genres : styles;
  if (head.length) list(head);
  else word('Tracks');
  if (genres.length && styles.length) (word(',', true), word('style'), list(styles));
  const tags = of('tag');
  if (tags.length) (word('tagged'), list(tags));
  const by = of('artist');
  if (by.length) (word('by'), list(by));
  const labels = of('label');
  if (labels.length) (word('on'), list(labels));
  const albums = of('album');
  if (albums.length) (word('from the album'), list(albums));

  // The years: one span, from the earliest chip's start to the latest's end; a decade "the 1970s", a span "1970 to 1985".
  const years = of('year')
    .map(({ c, i }) => ({ i, span: c.reading.value.split('-').map(Number) as [number, number], decade: /^\d{3}0s$/.test(c.reading.label) }))
    .sort((a, b) => a.span[0] - b.span[0] || a.span[1] - b.span[1]);
  const yearTerm = (y: (typeof years)[number]) => {
    if (y.decade) word('the');
    term(y.i, y.span[0] === y.span[1] || y.decade ? chips[y.i].reading.label : `${y.span[0]} to ${y.span[1]}`);
  };
  if (years.length) {
    word('from');
    const last = years.reduce((a, b) => (b.span[1] > a.span[1] ? b : a));
    yearTerm(years[0]);
    for (const y of years.slice(1)) if (y !== last) (word(',', true), yearTerm(y));
    if (last !== years[0]) (word('to'), yearTerm(last));
  }

  // What it feels and sounds like, one clause: "that feels Calm and sounds like Miles Davis" ("Tracks that feel…").
  const plural = !head.length;
  const moods = of('mood');
  const like = all.filter(({ c }) => likes(c));
  if (moods.length || like.length) word('that');
  if (moods.length) (word(plural ? 'feel' : 'feels'), list(moods));
  if (moods.length && like.length) word('and');
  if (like.length) (word(plural ? 'sound like' : 'sounds like'), list(like, 'and'));
  return out;
}

/** A name for it, from its chips: "Jazz · like Bonobo · 1970s". */
export function nameOf(chips: readonly Chip[]): string {
  return chips.map((c) => (likes(c) ? `like ${c.reading.label}` : c.reading.label)).join(' · ');
}

/** The crates made here and not saved that what is being typed may mean (the owner, 2026-10-30: the empty page stays the
 *  regular one until typing starts): none for nothing typed; else those whose name holds every word typed so far, the
 *  last word as a beginning. */
export function recentFor<T extends { chips: readonly Chip[] }>(recent: readonly T[], typed: string): T[] {
  const words = typed.toLowerCase().split(/[\s,;]+/).filter(Boolean);
  if (!words.length) return [];
  return recent.filter((k) => {
    const name = nameOf(k.chips).toLowerCase();
    const parts = name.split(/[\s·]+/).filter(Boolean);
    return words.every((w, i) => (i === words.length - 1 ? parts.some((p) => p.startsWith(w)) : name.includes(w)));
  });
}

/** Where a cut at closeness `min` falls on the closeness curve (how many are at least that alike), and back. */
export const countAt = (curve: readonly number[], min: number) => curve.filter((a) => a >= min).length;
export const minAt = (curve: readonly number[], count: number) => (count > 0 && count <= curve.length ? curve[count - 1] : 0);
