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
  return text
    .normalize('NFKD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
}

/** How well `texts` match the query's words: 0 where one word starts none of their words; else the more whole words,
 *  the higher (a whole word above a word's start, as the server ranks). */
export function score(query: string[], ...texts: (string | null | undefined)[]): number {
  if (!query.length) return 1;
  const hay = texts.flatMap(words);
  let s = 0;
  for (const w of query) {
    if (hay.includes(w)) s += 2;
    else if (hay.some((h) => h.startsWith(w))) s += 1;
    else return 0;
  }
  return s;
}

/** Whether `texts` match every word of the query (each a word's start). */
export const matches = (query: string[], ...texts: (string | null | undefined)[]) => score(query, ...texts) > 0;

/** Which of an item's fields the query's words are in: its `matched` line (each field holding at least one word). */
export function matchedOn(query: string[], fields: { on: string; text: string | null | undefined }[]): Matched[] {
  return fields.flatMap((f) => (f.text && query.some((w) => words(f.text).some((h) => h.startsWith(w))) ? [{ on: f.on, text: f.text }] : []));
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
