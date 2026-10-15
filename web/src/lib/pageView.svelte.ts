// What the page shown now hands the pill's sheets (lib/TopPill, decided 2026-10-15): a page's view settings (order,
// layout, size) live in the View sheet (lib/ViewSheet) and its own filters in the Filter sheet (lib/FilterSheet), not
// on the page. The page measures its grid and keeps its list as these say; the sheets read and change them (most are
// prefs, remembered by the browser).
import type { AlbumLayout, CollectionOrder, CrateOrder, PlaylistOrder } from './prefs.svelte';

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

export const DIRECTIONS = [
  { value: false, label: 'Ascending', icon: 'arrowUp' as const },
  { value: true, label: 'Descending', icon: 'arrowDown' as const },
];

export const pageView = $state({
  /** The page's grid width, which its size steps depend on (lib/SizeControl). */
  width: 0,
  /** Playlists: favourites only; not remembered (as it was on the page). */
  favouritesOnly: false,
  /** The page's search (the pill's field): what its list is filtered by; empty again on another page. */
  query: '',
});

/** The pill's search field's words on a page that has one. */
export const searchFor = (page: string) =>
  page === 'playlists' ? 'Filter playlists' : page === 'collection' || page === 'incoming' ? 'Filter albums' : null;

/** The pages whose view settings the View sheet holds. */
export const hasView = (page: string) => ['playlists', 'collection', 'incoming', 'crates'].includes(page);

/** How many, in words. */
export const howMany = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
