// The page's addresses (shared/addresses.json; the owner, 2026-11-02: "a completely coherent and robust architecture"):
// the one place that reads or builds one. An address is `#/<page>`, then after `?` the panel's words (decision 0023:
// what it views, on which tab) and the page's own (the Library's composer, Discover's Spotify playlist, Find's words,
// Settings' answers from a sign-in elsewhere). Every page has one; none, or one no page has, is Home's (App writes it
// as Home's as it opens). Nothing else writes `#/` (tests/addresses.test.ts).

export const PAGES = ['home', 'playlists', 'library', 'discover', 'crates', 'insights', 'health', 'settings', 'requests', 'downloads', 'find', 'local'] as const;
export type PageId = (typeof PAGES)[number];
export const HOME: PageId = 'home';

/** The panel's words: what it views (one or more) and on which tab. */
export const PANEL_WORDS = ['view', 'tab'] as const;
/** A page's own words, kept as the panel opens and closes over it. */
export const PAGE_WORDS: Partial<Record<PageId, readonly string[]>> = {
  library: ['composer'],
  discover: ['spotify'],
  find: ['q'],
  settings: ['scrobble', 'spotify_error'],
};

export const isPage = (p: string | null | undefined): p is PageId => !!p && (PAGES as readonly string[]).includes(p);

/** A page's address, nothing after it. */
export const hrefOf = (page: PageId) => `#/${page}`;

/** The page an address names; none, or one no page has: null. */
export function pageIn(hash = location.hash): PageId | null {
  const p = hash.split('?')[0].replace(/^#\/?/, '');
  return isPage(p) ? p : null;
}

/** The page shown: the address's, or Home. */
export const pageHere = (): PageId => pageIn() ?? HOME;

/** An address's words after `?` (a copy: changing it changes no address). */
export const wordsIn = (hash = location.hash) => new URLSearchParams(hash.split('?')[1] ?? '');

/** One of the shown address's words. */
export const wordHere = (key: string) => wordsIn().get(key);

type Words = URLSearchParams | Record<string, string | readonly string[] | null | undefined>;

/** `page`'s address with `words` after it (a list: the word once for each; empty or none: left out). */
export function addressOf(page: PageId, words?: Words): string {
  const q = new URLSearchParams();
  const entries = words instanceof URLSearchParams ? [...words] : Object.entries(words ?? {});
  for (const [k, v] of entries) for (const x of typeof v === 'string' ? [v] : (v ?? [])) if (x) q.append(k, x);
  const s = q.toString();
  return s ? `${hrefOf(page)}?${s}` : hrefOf(page);
}

/** `page` with `subject` viewed in the panel. */
export const viewOn = (page: PageId, subject: string) => addressOf(page, { view: subject });

/** The address that opens `subject` in the panel over `page`, for anyone who can reach this server (a share). */
export const linkTo = (page: PageId, subject: string) => `${location.origin}/${viewOn(page, subject)}`;

/** The shown page's address with its words changed by `change` (the panel's and its own alike). */
export function hereWith(change: (w: URLSearchParams) => void): string {
  const w = wordsIn();
  change(w);
  return addressOf(pageHere(), w);
}

/** The shown page's own words (not the panel's): what an address the panel builds keeps. */
export function ownWordsHere(): URLSearchParams {
  const w = wordsIn();
  const out = new URLSearchParams();
  for (const k of PAGE_WORDS[pageHere()] ?? []) for (const v of w.getAll(k)) out.append(k, v);
  return out;
}
