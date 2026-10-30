// One search, the browser's side (decision 0072): the words read as the server reads them (search::words; the examples
// in tests/fixtures/search-words.json hold both to it), one result shape for every kind, and the pages' own matchers
// registered so the global search reaches them. Pure but for the registry.

/** What a search found: what the panel opens (`subject`), how it shows, and why it is there (`matched`). */
export type SearchResult = {
  kind: string;
  subject: string;
  title: string;
  sub?: string | null;
  cover?: string | null;
  /** The artist, where a result needs it apart from its line (an album not in the library: to look it up). */
  artist?: string | null;
  /** An album not in the library: its page on Bandcamp, where an open found it (0.9.5). */
  bandcamp?: string | null;
  /** An album not in the library, asked for: where it stands (lib/CoverStatus). */
  request?: import('./api').RequestMark | null;
  /** A classical record's or track's composers (0.10), said on its line. */
  composers?: { key: string; name: string }[];
  /** Known elsewhere (decision 0076): why Cratarr knows it (on your Spotify, missing from an artist you have,
   *  suggested, requested). */
  known?: string[];
  score: number;
  matched: Matched[];
};
/** Why a result is there: on its name, an artist, an album, a track, a tag…; the text that matched; how many (a
 *  playlist: its tracks that match). */
export type Matched = { on: string; text: string; count?: number };

/** A text's words as search reads them: compatibility-decomposed (ﬁ, ＡＢＣ, Ⅳ), its marks dropped (é, ö, ç), case
 *  folded, everything but letters and numbers a gap. As SQLite's `unicode61 remove_diacritics 2` tokenizes. */
export function words(text: string | null | undefined): string[] {
  if (!text) return [];
  let ws = read.get(text);
  if (!ws) {
    ws = text
      .normalize('NFKD')
      .replace(/\p{M}+/gu, '')
      .toLowerCase()
      .split(/[^\p{L}\p{N}]+/u)
      .filter(Boolean);
    if (read.size >= READ_MOST) read.clear();
    read.set(text, ws);
  }
  return ws;
}
/** Texts as read before: a list is searched again at every key, and reading its texts was most of the work. */
const read = new Map<string, string[]>();
const READ_MOST = 200_000;

/** How many typos a query's word may have (the owner, 2026-10-26; Meilisearch's measure): none under five letters,
 *  one from five, two from nine; none in a word with a figure in it (a year, a catalogue number). The server allows
 *  the same (the shared examples, tests/fixtures/search-typos.json). */
export function typoBudget(w: string): number {
  if (/\p{N}/u.test(w)) return 0;
  return w.length >= 9 ? 2 : w.length >= 5 ? 1 : 0;
}

/** The edits from `a` to `b` (a letter added, dropped or changed, or two side by side swapped), or `most` + 1 as soon
 *  as it is clear there are more than `most`. */
function edits(a: string, b: string, most: number): number {
  if (Math.abs(a.length - b.length) > most) return most + 1;
  let before: number[] = [];
  let prev = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    let least = i;
    for (let j = 1; j <= b.length; j++) {
      let d = Math.min(prev[j] + 1, row[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) d = Math.min(d, before[j - 2] + 1);
      row.push(d);
      least = Math.min(least, d);
    }
    if (least > most) return most + 1;
    [before, prev] = [prev, row];
  }
  return prev[b.length];
}

/** How a query's word meets a text's word: the `whole` word, its `start`, or (within its typos, its first letter
 *  right) the word or its start with `typos`; null where it does not. A start with typos is one as long as the query's
 *  word give or take a letter: a word still being typed. */
export function meet(w: string, h: string): { how: 'whole' | 'start' | 'typo'; typos: number } | null {
  if (h === w) return { how: 'whole', typos: 0 };
  if (h.startsWith(w)) return { how: 'start', typos: 0 };
  const most = typoBudget(w);
  if (!most || h[0] !== w[0]) return null;
  let best = most + 1;
  for (const k of new Set([h.length, w.length - 1, w.length, w.length + 1])) if (k > 1 && k <= h.length) best = Math.min(best, edits(w, h.slice(0, k), most));
  return best <= most ? { how: 'typo', typos: best } : null;
}
const better = (m: { how: string; typos: number }) => (m.how === 'whole' ? 3 : m.how === 'start' ? 2 : 1 - m.typos / 10);
/** `meet` as worked out before, for the last few query words: a list holds the same words many times over. */
const met = new Map<string, Map<string, ReturnType<typeof meet>>>();
function meeting(w: string, h: string): ReturnType<typeof meet> {
  let of = met.get(w);
  if (!of) {
    if (met.size >= 16) met.clear();
    met.set(w, (of = new Map()));
  }
  let m = of.get(h);
  if (m === undefined) of.set(h, (m = meet(w, h)));
  return m;
}

/** How `texts` match the query's words: each word by its best meeting with their words (`meet`), every word needed
 *  (null where one meets none); `score` the more whole words the higher (a whole word 2, a start 1, with typos ½, as
 *  the server ranks), and the typos needed. */
export function match(query: string[], ...texts: (string | null | undefined)[]): { score: number; typos: number } | null {
  if (!query.length) return { score: 1, typos: 0 };
  const hay = texts.flatMap(words);
  let score = 0;
  let typos = 0;
  for (const w of query) {
    let best: ReturnType<typeof meet> = null;
    for (const h of hay) {
      const m = meeting(w, h);
      if (m && (!best || better(m) > better(best))) best = m;
      if (best?.how === 'whole') break;
    }
    if (!best) return null;
    score += best.how === 'whole' ? 2 : best.how === 'start' ? 1 : 0.5;
    typos += best.typos;
  }
  return { score, typos };
}

/** How well `texts` match the query's words (`match`'s score); 0 where they do not. */
export const score = (query: string[], ...texts: (string | null | undefined)[]): number => match(query, ...texts)?.score ?? 0;

/** How relevant a result is, for everywhere's order (the owner, 2026-10-26), compared with `byRelevance`, the first
 *  that differs deciding (as Meilisearch's rules):
 *  1. a direct hit (its own name holds the words) above a deep one (found through what it holds: an artist by an
 *     album, a playlist by a track);
 *  2. fewer typos;
 *  3. the whole name the words exactly (Jellyfin's exact match);
 *  4. more whole words (`score`);
 *  5. the name starting with the first word;
 *  6. how much of the name the words make up ("player" is all of a setting called Player, a little of a long title). */
export function relevance(query: string[], r: SearchResult): number[] {
  const own = match(query, r.title);
  const text = own ? r.title : (r.matched.find((m) => match(query, m.text))?.text ?? '');
  const m = own ?? match(query, text) ?? { score: 0, typos: 0 };
  const name = words(text);
  const exact = !m.typos && name.length === query.length && [...name].sort().join(' ') === [...query].sort().join(' ');
  const starts = !!query[0] && !!name[0] && !!meet(query[0], name[0]);
  const length = (ws: string[]) => ws.reduce((n, w) => n + w.length, 0);
  return [own ? 1 : 0, -m.typos, exact ? 1 : 0, m.score, starts ? 1 : 0, Math.min(1, length(query) / Math.max(1, length(name)))];
}
export function byRelevance(a: number[], b: number[]): number {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return b[i] - a[i];
  return 0;
}

/** Whether `texts` match every word of the query (each a word or its start, within its typos). */
export const matches = (query: string[], ...texts: (string | null | undefined)[]) => match(query, ...texts) !== null;

/** Which of an item's fields the query's words are in: its `matched` line (each field holding at least one word). */
export function matchedOn(query: string[], fields: { on: string; text: string | null | undefined }[]): Matched[] {
  return fields.flatMap((f) => (f.text && query.some((w) => words(f.text).some((h) => meet(w, h))) ? [{ on: f.on, text: f.text }] : []));
}

/** A page whose items only the browser holds (Settings, Health, Requests, Downloads, Insights): its finder, for the
 *  global search. */
export type Provider = { kind: string; label: string; find: (query: string[]) => SearchResult[] };
const providers = new Map<string, Provider>();
/** Registers a page's finder; the returned function takes it away (a page's teardown). */
export function provide(p: Provider): () => void {
  providers.set(p.kind, p);
  return () => {
    if (providers.get(p.kind) === p) providers.delete(p.kind);
  };
}
/** The registered finders, for the global search. */
export const provided = (): Provider[] => [...providers.values()];
