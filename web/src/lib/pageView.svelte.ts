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

/** An album's least width per size step (prefs.albumZoom): rows need room for their lines, covers less. */
export const ALBUM_WIDTHS: Record<AlbumLayout, readonly number[]> = {
  rows: [240, 280, 340, 420, 520, 680],
  grid: [110, 140, 170, 210, 260, 340],
};

export const ALBUM_LAYOUTS = [
  { value: 'rows' as const, label: 'Rows', icon: 'list' as const },
  { value: 'grid' as const, label: 'Covers', icon: 'layoutGrid' as const },
];

// Each order ascending as named (A–Z, oldest first, fewest first); a list's arrow turns it round.
export const ALBUM_ORDERS: { value: CollectionOrder; label: string }[] = [
  { value: 'artist', label: 'Artist' },
  { value: 'title', label: 'Title' },
  { value: 'year', label: 'Year' },
  { value: 'added', label: 'Added' },
];

export const PLAYLIST_ORDERS: { value: PlaylistOrder; label: string }[] = [
  { value: 'name', label: 'Name' },
  { value: 'tracks', label: 'Tracks' },
  { value: 'playing', label: 'Playing first' },
];

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
