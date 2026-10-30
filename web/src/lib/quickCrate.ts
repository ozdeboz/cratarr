// Quick crates (0.11; the owner, 2026-10-29 and 2026-10-30): a crate made from a few typed terms, each a chip. A
// genre, style, mood, tag, label, album or year (or decade) narrows it; an artist means "sounds like" them, the chip
// switching to "by" them, their own tracks left out unless kept. Filtered first, then ranked by how alike, cut where
// it stops being close (the closeness curve's handle loosens it). A word that reads several ways takes its best
// reading, its kind shown, the others a tap away. Pure: terms in, chips and a crate's conditions out.
import type { Condition } from './api';
import { byRelevance, relevance, words, type SearchResult } from './search';

export type ChipKind = 'artist' | 'album' | 'label' | 'genre' | 'style' | 'mood' | 'tag' | 'year';
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

const likes = (c: Chip) => c.reading.kind === 'artist' && c.how === 'like' && !!c.reading.id;

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
    if (r.kind === 'year') {
      const [a, b] = r.value.split('-').map(Number);
      years = years ? [Math.min(years[0], a), Math.max(years[1], b)] : [a, b];
    } else if (r.kind === 'artist') {
      if (!likes(c)) add('artist', r.value);
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

/** A name for it, from its chips: "Jazz · like Bonobo · 1970s". */
export function nameOf(chips: readonly Chip[]): string {
  return chips.map((c) => (likes(c) ? `like ${c.reading.label}` : c.reading.label)).join(' · ');
}

/** Where a cut at closeness `min` falls on the closeness curve (how many are at least that alike), and back. */
export const countAt = (curve: readonly number[], min: number) => curve.filter((a) => a >= min).length;
export const minAt = (curve: readonly number[], count: number) => (count > 0 && count <= curve.length ? curve[count - 1] : 0);
