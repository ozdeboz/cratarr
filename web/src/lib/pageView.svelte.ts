// What the page shown now hands the pill's sheets (lib/TopPill, decided 2026-10-15): a page's view settings (order,
// layout, size) live in the View sheet (lib/ViewSheet) and its own filters in the Filter sheet (lib/FilterSheet), not
// on the page. The page measures its grid and keeps its list as these say; the sheets read and change them (most are
// prefs, remembered by the browser).
import { untrack } from 'svelte';
import type { IconName } from './Icon.svelte';
import type { AlbumLayout, CollectionOrder, CrateOrder, PlaylistOrder } from './prefs.svelte';

/** The page's own button at the pill's end: a + where it makes something, or with things ticked their menu (⋮). */
/** `progress`: a ring around the icon, 0 to 1, while work goes on (the Downloads page's sync, decision 0038). */
export type PageAction = { icon: IconName; label: string; run: () => void; progress?: number | null };

/** A playlist chip's least width per size step (prefs.zoom). */
export const PLAYLIST_WIDTHS: readonly number[] = [110, 140, 170, 210, 260, 340];
/** A menu's playlists' least width per size step (Add to playlist, lib/PlaylistToggles): on a phone one column or two,
 *  never three (the owner: a toggle card narrower than that cannot show its name); three on a wider sheet. */
export const LIST_WIDTHS: readonly number[] = [160, 220, 340];

/** An album's least width per size step (prefs.albumZoom): rows need room for their lines, covers less. A table has one
 *  album a line, and no size steps (decision 0042). */
export const ALBUM_WIDTHS: Record<AlbumLayout, readonly number[]> = {
  rows: [240, 280, 340, 420, 520, 680],
  grid: [110, 140, 170, 210, 260, 340],
  table: [],
};

export const ALBUM_LAYOUTS = [
  { value: 'rows' as const, label: 'Rows', icon: 'list' as const },
  { value: 'grid' as const, label: 'Covers', icon: 'layoutGrid' as const },
  { value: 'table' as const, label: 'Table', icon: 'table' as const },
];

// Each order ascending as named (A–Z, oldest first, fewest first); a list's arrow turns it round. Plays, Last played
// and Rating (how you used them, decided 2026-10-16) start the other way round: most first.
export const ALBUM_ORDERS: { value: CollectionOrder; label: string }[] = [
  { value: 'artist', label: 'Artist' },
  { value: 'title', label: 'Title' },
  { value: 'year', label: 'Year' },
  { value: 'added', label: 'Added' },
  { value: 'plays', label: 'Plays' },
  { value: 'played', label: 'Last played' },
  { value: 'rating', label: 'Rating' },
];
/** The orders by how you used the albums (lib/albums albumUses): read when chosen, most first when chosen. */
export const USE_ORDERS: readonly CollectionOrder[] = ['plays', 'played', 'rating'];

export const PLAYLIST_ORDERS: { value: PlaylistOrder; label: string }[] = [
  { value: 'name', label: 'Name' },
  { value: 'tracks', label: 'Tracks' },
  { value: 'playing', label: 'Playing first' },
];

/** A menu's Add to playlist (lib/PlaylistToggles): the same orders, those the item is in ticked first. */
export const LIST_ORDERS: { value: PlaylistOrder; label: string }[] = PLAYLIST_ORDERS.map((o) =>
  o.value === 'playing' ? { ...o, label: 'Ticked first' } : o,
);

/** The playlists' order (the Playlists page's, a menu's Add to playlist): by name, by size, or those holding the item
 *  first (the playing track, or the menu's: `holds`); equals by name. */
export function playlistBy<T extends { name: string; count: number }>(order: PlaylistOrder, holds: (p: T) => boolean) {
  const byName = (a: T, b: T) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true });
  return (a: T, b: T) =>
    order === 'tracks' ? a.count - b.count || byName(a, b) : order === 'playing' ? Number(holds(b)) - Number(holds(a)) || byName(a, b) : byName(a, b);
}

export const CRATE_ORDERS: { value: CrateOrder; label: string }[] = [
  { value: 'own', label: 'As made' },
  { value: 'name', label: 'Name' },
];

/** View's Dates, on the album lists: how long ago, or the date. */
export const DATES = [
  { value: true, label: 'Relative' },
  { value: false, label: 'Absolute' },
];

export const DIRECTIONS = [
  { value: false, label: 'Ascending', icon: 'arrowUp' as const },
  { value: true, label: 'Descending', icon: 'arrowDown' as const },
];

export const pageView = $state({
  /** The page's grid width, which its size steps depend on (lib/SizeControl). */
  width: 0,
  /** Playlists, Library: favourites only; not remembered, the list's own (off again on another page). */
  favouritesOnly: false,
  /** Library: only Incoming's albums that would upgrade one in Music; not remembered either. */
  upgradesOnly: false,
  /** Library: only albums never played, and only those rated 4 stars or 5 (decided 2026-10-16); not remembered. */
  unplayedOnly: false,
  topRatedOnly: false,
  /** The page's search (the pill's field): what its list is filtered by; empty again on another page. */
  query: '',
  /** A library of the page's list is switched off (lib/AlbumShelf): the pill's funnel is lit. */
  narrowed: false,
  /** The page's own button at the pill's end (usePageAction), while it shows. */
  action: null as PageAction | null,
});

/** Gives the pill's last button to the calling page while it shows (an effect: it follows what `get` reads). */
export function usePageAction(get: () => PageAction | null) {
  $effect(() => {
    const mine = get();
    pageView.action = mine;
    // Gone with the page, unless the next page has set its own already.
    return () => {
      if (untrack(() => pageView.action?.run) === mine?.run) pageView.action = null;
    };
  });
}

/** The pill's search field's words on a page that has one. */
export const searchFor = (page: string) =>
  page === 'playlists' ? 'Filter playlists' : page === 'library' || page === 'downloads' ? 'Filter albums' : null;

/** The pages whose view settings the View sheet holds. */
export const hasView = (page: string) => ['playlists', 'library', 'crates', 'downloads'].includes(page);

/** How many, in words. */
export const howMany = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
