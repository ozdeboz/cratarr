// The global search's state (decision 0072; the owner, 2026-10-25, mockup 3): what the words find everywhere, asked
// while any search is on, so its count shows beside the page's before it is looked at; and which the page or panel
// shows, its own results or everywhere's (the globe beside the search, lib/TopPill). The library's kinds and what is not in it from the
// server, the pages and Settings' parts from the browser (lib/everywhere).
import { search, searchBeyond, type BeyondAlbum, type SearchKind, type ServerSearch } from './api';
import { pageView } from './pageView.svelte';
import { isSpotifyView } from './discoverModes';
import { prefs } from './prefs.svelte';
import { byRelevance, provided, relevance, words, type SearchResult } from './search';
import { leaveOnArrival, onArrival } from './navStep.svelte';
import { keep } from './searchKept';

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
  // The harvest's words (0.9.7; the owner, 2026-10-26: a group per kind), each opening its crate.
  { kind: 'style', label: 'Styles' },
  { kind: 'mood', label: 'Moods' },
  { kind: 'tag', label: 'Tags' },
];
/** Known elsewhere (decision 0076): what Cratarr keeps of music not in the library (0.9.13's server adds the last
 *  three; one before it leaves them out). */
const ELSEWHERE: { kind: SearchKind; label: string }[] = [
  { kind: 'missing', label: 'Missing from your artists' },
  { kind: 'spotify', label: 'On your Spotify' },
  { kind: 'suggested', label: 'Suggested' },
  { kind: 'requested', label: 'Requested' },
  { kind: 'excluded', label: 'Left out by Discover' },
];
/** The kinds a page shows itself: left out of its everywhere (lib/GlobalResults); Discover's Spotify lists none. */
export const pageKinds = (page: string): string[] => (page === 'discover' && isSpotifyView(prefs.discoverMode) ? [] : (PAGE_KINDS[page] ?? []));
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
  /** Beyond (decision 0076): what MusicBrainz (and Discogs, 0.9.13's server) find for the settled words. */
  beyond: null as { q: string; albums: BeyondAlbum[] } | null,
  /** Show all (decision 0076): the one kind shown whole, as far as it has come. */
  all: null as string | null,
  allFound: null as { q: string; kind: string; items: Found[]; total: number; done: boolean } | null,
});

/** Beyond is asked once the words settle (decision 0076: about half a second, three letters at least), each query's
 *  answer kept, so typing back and forth asks nothing again. */
const BEYOND_MS = 500;
const beyondKept = new Map<string, BeyondAlbum[]>();
/** The server's answers kept by their words, and Show all's as far as it came (0.10; the owner, 2026-10-30: back to
 *  a search, everywhere's results came back only after a while): back shows them at once, asked again behind them. */
const foundKept = new Map<string, ServerSearch>();
const allKept = new Map<string, NonNullable<typeof globalSearch.allFound>>();

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
    const kept = foundKept.get(q);
    if (kept) globalSearch.found = { q, r: kept };
    let gone = false;
    const t = setTimeout(() => {
      search(q, [...LIBRARY, ...ELSEWHERE].map((k) => k.kind), EACH).then(
        (r) => {
          keep(foundKept, q, r);
          if (!gone) globalSearch.found = { q, r };
        },
        () => {},
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  $effect(() => {
    const q = pageView.query.trim();
    if (q.replace(/\s/g, '').length < 3) return void (globalSearch.beyond = null);
    const kept = beyondKept.get(q.toLowerCase());
    if (kept) return void (globalSearch.beyond = { q, albums: kept });
    let gone = false;
    const t = setTimeout(() => {
      searchBeyond(q).then(
        ({ albums }) => {
          beyondKept.set(q.toLowerCase(), albums);
          if (!gone) globalSearch.beyond = { q, albums };
        },
        () => {},
      );
    }, BEYOND_MS);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  // Show all: the kind's first page, again for new words.
  $effect(() => {
    const kind = globalSearch.all;
    const q = pageView.query.trim();
    if (!kind || !q) return void (globalSearch.allFound = null);
    const kept = allKept.get(`${q}
${kind}`);
    if (kept) return void (globalSearch.allFound = kept);
    void loadAll(q, kind, true);
  });
});

/** How many a Show all asks for at a time. */
const PAGE = 50;
/** Show all's next page (the first, `fresh`): the server's kinds by offset, the browser's whole at once. */
export async function loadAll(q: string, kind: string, fresh = false) {
  const was = globalSearch.allFound;
  if (!fresh && (!was || was.q !== q || was.kind !== kind || was.done)) return;
  const have = fresh ? [] : was!.items;
  const provider = provided().find((p) => p.kind === kind);
  if (provider) {
    const items = provider.find(words(q)).sort((a, b) => b.score - a.score) as Found[];
    globalSearch.allFound = { q, kind, items, total: items.length, done: true };
    keep(allKept, `${q}
${kind}`, globalSearch.allFound);
    return;
  }
  try {
    const r = await search(q, [kind as SearchKind], PAGE, have.length);
    const g = r.groups[kind as SearchKind];
    const page = (g?.items ?? []) as Found[];
    const now = globalSearch.allFound;
    if (!fresh && (now?.q !== q || now.kind !== kind)) return;
    // A server that pages not (before 0.9.13) answers the first page again: nothing more then.
    const repeat = !!have.length && !!page.length && have.some((x) => x.subject === page[0].subject && x.track === page[0].track);
    const items = repeat ? have : [...have, ...page];
    const total = g?.total ?? items.length;
    globalSearch.allFound = { q, kind, items, total, done: repeat || !page.length || items.length >= total };
    keep(allKept, `${q}
${kind}`, globalSearch.allFound);
  } catch {
    if (globalSearch.allFound) globalSearch.allFound.done = true;
  }
}
/** Opens a group whole (Show all), a step of the history so back returns to every group. */
export function showAll(kind: string) {
  history.pushState({ ...(history.state ?? {}), searchAll: kind }, '');
  globalSearch.all = kind;
}
if (typeof window !== 'undefined') addEventListener('popstate', () => (globalSearch.all = (history.state?.searchAll as string | undefined) ?? null));

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
  // Beyond (decision 0076): last, each album once, none the library or what Cratarr knows already has.
  const known = new Set(
    [...server(LIBRARY), ...server(ELSEWHERE)].flatMap((g) => g.items.flatMap((it) => [it.subject.replace(/^(missing|excluded):/, ''), `${(it.artist ?? it.sub ?? '').toLowerCase()}|${it.title.toLowerCase()}`])),
  );
  const b = globalSearch.beyond?.q === q ? globalSearch.beyond.albums : [];
  const beyond: Found[] = b
    .filter((a) => !a.have && !(a.release_group && known.has(a.release_group)) && !known.has(`${a.artist.toLowerCase()}|${a.title.toLowerCase()}`))
    .map((a) => ({
      kind: 'beyond',
      subject: `beyond:${a.release_group ?? a.discogs ?? `${a.artist}|${a.title}`}`,
      title: a.title,
      artist: a.artist,
      sub: [a.artist, a.year, a.sources.map((x) => (x === 'discogs' ? 'Discogs' : 'MusicBrainz')).join(' · ')].filter(Boolean).join(' · '),
      cover: a.cover,
      bandcamp: a.bandcamp ?? null,
      request: a.request ?? null,
      score: 0,
      matched: [],
    }));
  const beyondGroup: FoundGroup[] = beyond.length && !except.includes('beyond') ? [{ kind: 'beyond', label: 'Beyond', items: beyond.slice(0, EACH), total: beyond.length }] : [];
  return [...ranked([...server(LIBRARY), ...here]), ...ranked(server(ELSEWHERE)), ...beyondGroup];
}
/** Beyond's albums as asked (to open one: its release group, its year). */
export const beyondAlbum = (subject: string) =>
  globalSearch.beyond?.albums.find((a) => `beyond:${a.release_group ?? a.discogs ?? `${a.artist}|${a.title}`}` === subject) ?? null;

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
 *  its contents: it opens narrowed to them (decision 0072). Gone once what it opened shows (lib/navStep; the owner,
 *  2026-10-30: the results vanished and the page flashed before the panel came). */
export function leaveSearch(deep: boolean) {
  rememberSearch();
  leaveOnArrival({ search: deep ? 'page' : 'clear' });
}
onArrival((l) => {
  if (l.restore) {
    pageView.query = l.restore.q;
    globalSearch.scope = l.restore.scope;
  } else if (l.search === 'clear') pageView.query = '';
  else if (l.search === 'page') globalSearch.scope = 'page';
});
if (typeof window !== 'undefined') {
  // Back to a search: its words and scope as they were, over the page's own emptying (App), once where back went
  // shows; the server's answer for them kept, so they are there at once.
  addEventListener('popstate', () => {
    const kept = history.state?.search as { q: string; scope: 'page' | 'everywhere' } | undefined;
    if (kept) leaveOnArrival({ restore: kept }, true);
  });
}
