// What the page shown now hands the pill's sheets (lib/TopPill, decided 2026-10-15): a page's view settings (order,
// layout, size) live in the View sheet (lib/ViewSheet) and its own filters in the Filter sheet (lib/FilterSheet), not
// on the page. The page measures its grid and keeps its list as these say; the sheets read and change them (most are
// prefs, remembered by the browser).
import type { AlbumLayout } from './prefs.svelte';

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

export const ALBUM_ORDERS = [
  { value: 'artist' as const, label: 'Artist' },
  { value: 'title' as const, label: 'Title' },
  { value: 'year' as const, label: 'Year' },
  { value: 'added' as const, label: 'Added', title: 'Recently added first' },
];

export const pageView = $state({
  /** The page's grid width, which its size steps depend on (lib/SizeControl). */
  width: 0,
  /** Playlists: favourites only; not remembered (as it was on the page). */
  favouritesOnly: false,
});

/** The pages whose view settings the View sheet holds. */
export const hasView = (page: string) => page === 'playlists' || page === 'collection' || page === 'incoming';
