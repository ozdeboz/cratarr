// One search, the browser's side (decision 0072): the words read as the server reads them (search::words; the examples
// in tests/fixtures/search-words.json hold both to it), one result shape for every kind, and the pages' own matchers
// registered so the global search reaches them. Pure but for the registry and the matcher it is given.
//
// 0.12: what matches is the server's own matcher (lib/matcher: the rules crate as WebAssembly, held to the server's
// vectors), which lib/matcherLoad fetches as soon as the page is idle, or at once when a search comes first. Separators
// do not count, aliases count, a few typos are forgiven; exact before alias before prefix before fuzzy. Until it is in
// (the first moment of a cold start), nothing is left out by a search: the lists read again when it arrives.

import type { Hit, TextMatcher } from './matcher';

/** What a search found: what the panel opens (`subject`), how it shows, and why it is there (`matched`). */
export type SearchResult = {
  kind: string;
  subject: string;
  title: string;
  sub?: string | null;
  cover?: string | null;
  /** The artist, where a result needs it apart from its line (an album not in the library: to look it up; a track, 0.12). */
  artist?: string | null;
  /** A track's album, its id and its length (0.12): for its menu, apart from its line. */
  album?: string | null;
  album_id?: string | null;
  duration_secs?: number | null;
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

/** The matcher once it is in (lib/matcherLoad); null until then. */
let matcher: TextMatcher | null = null;
/** Asked for at once when a search comes before the page was idle (lib/matcherLoad sets it). */
let wanted: (() => void) | null = null;
export function useMatcher(m: TextMatcher | null) {
  matcher = m;
}
export function onMatcherWanted(f: (() => void) | null) {
  wanted = f;
}
export const matcherReady = () => matcher !== null;

/** A hit as `match` answers: a score the stronger the higher (exact 4, alias 3.5, prefix 2 to 3, fuzzy 1 to 2: its
 *  strength, then its own score), and a guess counted as a typo, for relevance's order. */
const fromHit = (h: Hit) => ({ score: { exact: 3, alias: 2.5, prefix: 2, fuzzy: 1 }[h.how] + h.score, typos: h.how === 'fuzzy' ? 1 : 0 });
const texted = (texts: (string | null | undefined)[]) => texts.filter((t): t is string => !!t && !!t.trim());

/** How `texts` match the query's words, all the texts as one name and each also its own (lib/matcher); null where they
 *  do not. Before the matcher is in, every text passes (score 0): nothing is left out until it can be judged. */
export function match(query: string[], ...texts: (string | null | undefined)[]): { score: number; typos: number } | null {
  if (!query.length) return { score: 1, typos: 0 };
  if (!matcher) {
    wanted?.();
    return { score: 0, typos: 0 };
  }
  const hit = matcher.hit(query.join(' '), texted(texts));
  return hit ? fromHit(hit) : null;
}

/** How well `texts` match the query's words (`match`'s score); 0 where they do not. */
export const score = (query: string[], ...texts: (string | null | undefined)[]): number => match(query, ...texts)?.score ?? 0;

/** How relevant a result is, for everywhere's order (the owner, 2026-10-26), compared with `byRelevance`, the first
 *  that differs deciding (as Meilisearch's rules):
 *  1. a direct hit (its own name holds the words) above a deep one (found through what it holds: an artist by an
 *     album, a playlist by a track);
 *  2. no guess above a guess (the matcher's fuzzy);
 *  3. the whole name the words exactly (the matcher's exact: separators aside, "hiphop" is Hip Hop);
 *  4. the stronger meeting, then the closer (`score`);
 *  5. the name starting with the first word;
 *  6. how much of the name the words make up ("player" is all of a setting called Player, a little of a long title). */
export function relevance(query: string[], r: SearchResult): number[] {
  const own = match(query, r.title);
  const text = own ? r.title : (r.matched.find((m) => match(query, m.text))?.text ?? '');
  const m = own ?? match(query, text) ?? { score: 0, typos: 0 };
  const name = words(text);
  const exact = m.score >= 4;
  const starts = !!query[0] && !!name[0] && name[0].startsWith(query[0]);
  const length = (ws: string[]) => ws.reduce((n, w) => n + w.length, 0);
  return [own ? 1 : 0, -m.typos, exact ? 1 : 0, m.score, starts ? 1 : 0, Math.min(1, length(query) / Math.max(1, length(name)))];
}
export function byRelevance(a: number[], b: number[]): number {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return b[i] - a[i];
  return 0;
}

/** What a result's match line calls each place its words were found (the owner, 2026-10-31: "Hip Hop Music · 5 tracks"
 *  said neither that it was a track nor why another line had no count). */
const MATCHED_ON: Record<string, string> = { track: 'Track', album: 'Album', artist: 'Artist', composer: 'Composer' };
/** A result's match line: where the words were found and how many (its first match not on its own name), the count
 *  always said when the server gives one: its tracks for a playlist, a crate or an album found by a track, its albums
 *  for an artist found by an album. Null when it was found by its name. */
export function matchLine(matched: readonly Matched[], kind = ''): string | null {
  const m = matched.find((x) => x.on !== 'name');
  if (!m) return null;
  const on = MATCHED_ON[m.on] ?? m.on.charAt(0).toUpperCase() + m.on.slice(1);
  if (m.count === undefined || m.count === null) return `${on}: ${m.text}`;
  const unit = kind === 'artist' && m.on === 'album' ? 'album' : 'track';
  return `${on}: ${m.text} · ${m.count.toLocaleString()} ${unit}${m.count === 1 ? '' : 's'}`;
}

/** How long a list still asking the server shows nothing before its loader (lib/Asking), so a quick answer shows none. */
export const ASK_DELAY = 300;
/** What a searched list shows (the owner, 2026-10-31: never "no X found" before the answer is in): its rows if it has
 *  any; else, while the server's answer for the words as they are now (`words`) is not in (`answeredFor`: the words
 *  last answered, or failed), the wait (lib/Asking); its empty line only after. */
export function listShows(rows: number, words: string, answeredFor: string | null | undefined): 'rows' | 'asking' | 'empty' {
  if (rows > 0) return 'rows';
  const q = words.trim();
  return q && answeredFor !== q ? 'asking' : 'empty';
}

/** Whether `texts` match the query (`match`). */
export const matches = (query: string[], ...texts: (string | null | undefined)[]) => match(query, ...texts) !== null;

/** Which of an item's fields the query is in: its `matched` line (each field the whole query, or one of its words,
 *  meets). */
export function matchedOn(query: string[], fields: { on: string; text: string | null | undefined }[]): Matched[] {
  const holds = (text: string) => !matcher || !!matcher.hit(query.join(' '), [text]) || query.some((w) => !!matcher!.hit(w, [text]));
  return fields.flatMap((f) => (f.text && query.length && holds(f.text) ? [{ on: f.on, text: f.text }] : []));
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
