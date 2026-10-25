// The global search's state (decision 0072; the owner, 2026-10-25, mockup 3): what the words find everywhere, asked
// while any search is on, so its count shows beside the page's before it is looked at; and which the page or panel
// shows, its own results or everywhere's (lib/SearchScope). The library's kinds and what is not in it from the
// server, the pages and Settings' parts from the browser (lib/everywhere).
import { search, type SearchKind, type ServerSearch } from './api';
import { pageView } from './pageView.svelte';
import { provided, words, type SearchResult } from './search';

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
/** A few of each kind. */
const EACH = 5;

export const globalSearch = $state({
  /** What the page or the panel shows while searching: its own results, or everywhere's. */
  scope: 'page' as 'page' | 'everywhere',
  /** The server's answer, for these words; null while asked (or none). */
  found: null as { q: string; r: ServerSearch } | null,
  /** Asked again after something changed what it finds (an exclusion taken back). */
  again: 0,
});

$effect.root(() => {
  // Asked a moment after the words stop changing; nothing for no words. No words: back to the page's own.
  $effect(() => {
    const q = pageView.query.trim();
    void globalSearch.again;
    globalSearch.found = null;
    if (!words(q).length) {
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
  const f = globalSearch.found?.q === q ? globalSearch.found.r : null;
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
  return [...server(LIBRARY), ...here, ...server(ELSEWHERE)];
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
