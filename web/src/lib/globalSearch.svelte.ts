// The global search's state (decision 0072; the owner, 2026-10-25, mockup 3): what the words find everywhere, asked
// while any search is on, so its count shows beside the page's before it is looked at; and which the page or panel
// shows, its own results or everywhere's (the globe beside the search, lib/TopPill). The library's kinds and what is not in it from the
// server, the pages and Settings' parts from the browser (lib/everywhere).
import { search, type SearchKind, type ServerSearch } from './api';
import { pageView } from './pageView.svelte';
import { prefs } from './prefs.svelte';
import { byRelevance, provided, relevance, words, type SearchResult } from './search';

export type Found = SearchResult & { track?: string };
export type FoundGroup = { kind: string; label: string; items: Found[]; total: number };

/** The server's kinds, the library's first, then what is not in it (decision 0072, 7). */
const LIBRARY: { kind: SearchKind; label: string }[] = [
  { kind: 'artist', label: 'Artists' },
  { kind: 'album', label: 'Albums' },
  { kind: 'track', label: 'Tracks' },
  { kind: 'playlist', label: 'Playlists' },
  { kind: 'crate', label: 'Crates' },
  { kind: 'genre', label: 'Genres' },
];
const ELSEWHERE: { kind: SearchKind; label: string }[] = [
  { kind: 'missing', label: 'Not in your library' },
  { kind: 'excluded', label: 'Left out by Discover' },
];
/** The kinds a page shows itself: left out of its everywhere (lib/GlobalResults); the Library's by what it shows. */
export const pageKinds = (page: string): string[] =>
  page === 'library' && prefs.libraryView !== 'albums' ? [prefs.libraryView === 'artists' ? 'artist' : 'track'] : (PAGE_KINDS[page] ?? []);
const PAGE_KINDS: Record<string, string[]> = {
  library: ['album'],
  playlists: ['playlist'],
  crates: ['crate', 'genre'],
  settings: ['setting'],
  requests: ['page:#/requests'],
  downloads: ['page:#/downloads'],
  insights: ['page:#/insights'],
};
/** A few of each kind. */
const EACH = 5;

export const globalSearch = $state({
  /** What the page or the panel shows while searching: its own results, or everywhere's. */
  scope: 'page' as 'page' | 'everywhere',
  /** The server's last answer: kept while new words are asked, so the results stay until the new ones are in (the
   *  owner, 2026-10-25); none without words. */
  found: null as { q: string; r: ServerSearch } | null,
  /** Asked again after something changed what it finds (an exclusion taken back). */
  again: 0,
});

$effect.root(() => {
  // Asked a moment after the words stop changing; nothing for no words. No words: back to the page's own.
  $effect(() => {
    const q = pageView.query.trim();
    void globalSearch.again;
    if (!words(q).length) {
      globalSearch.found = null;
      globalSearch.scope = 'page';
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      search(q, [...LIBRARY, ...ELSEWHERE].map((k) => k.kind), EACH).then(
        (r) => !gone && (globalSearch.found = { q, r }),
        () => {},
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
});

/** Everywhere's groups but the kinds the page already shows (`except`): the server's library kinds, the browser's
 *  (pages, Settings), then what is not in the library. Read reactively. */
export function globalGroups(except: readonly string[]): FoundGroup[] {
  const q = pageView.query.trim();
  const query = words(q);
  if (!query.length) return [];
  // The last answer, the words' own once it is in.
  const f = globalSearch.found?.r ?? null;
  const server = (kinds: typeof LIBRARY): FoundGroup[] =>
    f
      ? kinds
          .filter((k) => !except.includes(k.kind))
          .map((k) => ({ ...k, items: (f.groups[k.kind]?.items ?? []) as Found[], total: f.groups[k.kind]?.total ?? 0 }))
          .filter((g) => g.items.length)
      : [];
  const here: FoundGroup[] = provided()
    .filter((p) => !except.includes(p.kind))
    .map((p) => {
      const all = p.find(query).sort((a, b) => b.score - a.score);
      return { kind: p.kind, label: p.label, items: all.slice(0, EACH) as Found[], total: all.length };
    })
    .filter((g) => g.items.length);
  // Ordered by relevance (the owner, 2026-10-26): within a group, direct hits first; the groups by their best, so the
  // one with the hit leads (an album found by its title above its artist found by it). What is not in the library
  // stays after the rest. Sorting keeps the order given for equals.
  const ranked = (gs: FoundGroup[]) =>
    gs
      .map((g) => {
        const items = g.items.map((it) => ({ it, r: relevance(query, it) })).sort((a, b) => byRelevance(a.r, b.r));
        return { g: { ...g, items: items.map((x) => x.it) }, best: items[0].r };
      })
      .sort((a, b) => byRelevance(a.best, b.best))
      .map((x) => x.g);
  return [...ranked([...server(LIBRARY), ...here]), ...ranked(server(ELSEWHERE))];
}

/** Whether the server has answered these words (else everywhere's count and its "nothing" wait). */
export const globalAsked = () => globalSearch.found?.q === pageView.query.trim();

/** Everywhere chosen while searching: a page's or a panel's own content set aside for everywhere's. */
export const everywhere = () => globalSearch.scope === 'everywhere' && pageView.query.trim() !== '';

/** Before a tapped result goes there: the search kept with this step of the history, so back shows it again (the
 *  owner, 2026-10-25: "after clicking on a search result, clicking back should reopen those results"). */
export function rememberSearch() {
  const q = pageView.query.trim();
  if (q) history.replaceState({ ...(history.state ?? {}), search: { q, scope: globalSearch.scope } }, '');
}
/** A found one opened (a result, a page's own list): kept for back, then the words gone (the owner, 2026-10-25: "When
 *  clicking on a search result, the search input field should always go back to zero"), but where it was found deep, in
 *  its contents: it opens narrowed to them (decision 0072). */
export function leaveSearch(deep: boolean) {
  rememberSearch();
  if (deep) globalSearch.scope = 'page';
  else pageView.query = '';
}
if (typeof window !== 'undefined') {
  addEventListener('popstate', () => {
    const kept = history.state?.search as { q: string; scope: 'page' | 'everywhere' } | undefined;
    if (!kept) return;
    // After the page's own change has emptied its search (App: each page starts with it empty).
    setTimeout(() => {
      pageView.query = kept.q;
      globalSearch.scope = kept.scope;
    }, 0);
  });
}
