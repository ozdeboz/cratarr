// What the page shown now hands the pill's sheets (lib/TopPill, decided 2026-10-15): a page's view settings (order,
// layout, size) live in the View sheet (lib/ViewSheet) and its own filters in the Filter sheet (lib/FilterSheet), not
// on the page. The page measures its grid and keeps its list as these say; the sheets read and change them (most are
// prefs, remembered by the browser).
import { untrack } from 'svelte';
import type { IconName } from './Icon.svelte';
import type { AlbumLayout, CollectionOrder, CrateOrder, PanelKind, PlaylistOrder } from './prefs.svelte';

/** A list's order field (decision 0060: one name per fact). `most`: it starts most (or newest, or best) first when
 *  chosen, a count or a date of how it was used; the list's arrow still turns it round. */
export type OrderField<K extends string = string> = { value: K; label: string; most?: true };

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
  // Flow sizes itself from the room it has (lib/CoverFlow).
  flow: [],
};

export const ALBUM_LAYOUTS = [
  { value: 'rows' as const, label: 'Rows', icon: 'list' as const },
  { value: 'grid' as const, label: 'Covers', icon: 'layoutGrid' as const },
  { value: 'table' as const, label: 'Table', icon: 'table' as const },
  // Digging through the albums (the owner, 2026-10-23): a coverflow, lib/CoverFlow.
  { value: 'flow' as const, label: 'Flow', icon: 'albums' as const },
];

// Each order ascending as named (A–Z, oldest first, fewest first); a list's arrow turns it round. Plays, Last played
// and Rating (how you used them, decided 2026-10-16) start the other way round: most first; Quality best first.
export const ALBUM_ORDERS: OrderField<CollectionOrder>[] = [
  { value: 'artist', label: 'Artist' },
  { value: 'title', label: 'Title' },
  { value: 'year', label: 'Year' },
  { value: 'added', label: 'Added' },
  { value: 'plays', label: 'Plays', most: true },
  { value: 'played', label: 'Last played', most: true },
  { value: 'rating', label: 'Rating', most: true },
  { value: 'quality', label: 'Quality', most: true },
];
/** The orders by how you used the albums (lib/albums albumUses): read when chosen. */
export const USE_ORDERS: readonly CollectionOrder[] = ['plays', 'played', 'rating'];
/** An album's quality, to order by: lossless above lossy, then its lowest bit rate (decision 0060). */
const LOSSLESS = new Set(['FLAC', 'ALAC', 'WAV', 'AIFF', 'APE', 'WV', 'WAVPACK', 'DSF', 'DFF']);
export const isLossless = (format: string | null | undefined) => !!format && LOSSLESS.has(format.toUpperCase());
export const qualityOf = (a: { format: string | null; bitrate_kbps: number | null }) =>
  (isLossless(a.format) ? 100_000 : 0) + (a.bitrate_kbps ?? 0);

/** The Playlists page's layouts: its chips, or a table (decision 0042). */
export const PLAYLIST_LAYOUTS = [
  { value: 'chips' as const, label: 'Chips', icon: 'layoutGrid' as const },
  { value: 'table' as const, label: 'Table', icon: 'table' as const },
];

export const PLAYLIST_ORDERS: OrderField<PlaylistOrder>[] = [
  { value: 'name', label: 'Name' },
  { value: 'tracks', label: 'Tracks' },
  { value: 'playing', label: 'Playing first' },
  { value: 'missing', label: 'Missing', most: true },
];

/** A menu's Add to playlist (lib/PlaylistToggles): the same orders, those the item is in ticked first. */
export const LIST_ORDERS: OrderField<PlaylistOrder>[] = PLAYLIST_ORDERS.filter((o) => o.value !== 'missing').map((o) =>
  o.value === 'playing' ? { ...o, label: 'Ticked first' } : o,
);

/** The playlists' order (the Playlists page's, a menu's Add to playlist): by name, by size, or those holding the item
 *  first (the playing track, or the menu's: `holds`); equals by name. */
export function playlistBy<T extends { name: string; count: number; dead?: number }>(order: PlaylistOrder, holds: (p: T) => boolean) {
  const byName = (a: T, b: T) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true });
  return (a: T, b: T) =>
    order === 'tracks' ? a.count - b.count || byName(a, b)
    : order === 'missing' ? (a.dead ?? 0) - (b.dead ?? 0) || byName(a, b)
    : order === 'playing' ? Number(holds(b)) - Number(holds(a)) || byName(a, b) : byName(a, b);
}

export const CRATE_ORDERS: OrderField<CrateOrder>[] = [
  { value: 'own', label: 'As made' },
  { value: 'name', label: 'Name' },
  { value: 'tracks', label: 'Tracks' },
  { value: 'length', label: 'Length' },
  { value: 'unplayed', label: 'Unplayed', most: true },
  { value: 'newWeek', label: 'New this week', most: true },
];

/** A list of tracks' fields (decision 0060): every list of tracks offers all its rows know. Its own order is named by
 *  what it is: Playlist order, Track number (an album), Newest first (a crate), Newest album first (an artist). */
export type TrackOrder = 'own' | 'title' | 'artist' | 'albumArtist' | 'album' | 'year' | 'track' | 'length' | 'tempo' | 'plays' | 'lastPlayed' | 'added';
export function trackOrders(kind: PanelKind): OrderField<TrackOrder>[] {
  const own = { playlist: 'Playlist order', album: 'Track number', crate: 'Newest first', artist: 'Newest album first' }[kind];
  return [
    { value: 'own', label: own },
    { value: 'title', label: 'Title' },
    { value: 'artist', label: 'Artist' },
    { value: 'albumArtist', label: 'Album artist' },
    ...(kind === 'album' ? [] : [{ value: 'album' as const, label: 'Album' }]),
    { value: 'year', label: 'Year' },
    ...(kind === 'album' ? [] : [{ value: 'track' as const, label: 'Track number' }]),
    { value: 'length', label: 'Length' },
    { value: 'tempo', label: 'Tempo' },
    { value: 'plays', label: 'Plays', most: true },
    { value: 'lastPlayed', label: 'Last played', most: true },
    { value: 'added', label: 'Added', most: true },
  ];
}

/** Each kind of list's own switches in the Filter sheet, after the conditions (decisions 0060 and 0061). A kind of list
 *  is a page's id or a panel kind. `incoming`: only where a library is used as Incoming. */
export type ListSwitch = { key: string; label: string; incoming?: true };
const TRACK_SWITCHES: ListSwitch[] = [
  { key: 'favourites', label: 'Favourites only' },
  { key: 'unplayed', label: 'Not played yet' },
  { key: 'missing', label: 'Hide missing tracks' },
];
export const SWITCHES: Record<string, ListSwitch[]> = {
  library: [
    { key: 'favourites', label: 'Favourites only' },
    { key: 'unplayed', label: 'Not played yet' },
    { key: 'topRated', label: '4 stars and up' },
    // Not Lossless only, Upgrades waiting or Duplicates waiting (the owner, 2026-10-24: not filter options).
  ],
  playlists: [
    { key: 'favourites', label: 'Favourites only' },
    { key: 'mine', label: 'Mine only' },
  ],
  // In the panel a playlist's only (the owner, 2026-10-24): not an album's, a crate's or an artist's.
  playlist: TRACK_SWITCHES,
};

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
  /** The page's search (the pill's field): what its list is filtered by; empty again on another page. */
  query: '',
  /** A library of the page's list is switched off (lib/AlbumShelf): the pill's funnel is lit. */
  narrowed: false,
  /** How many of the page's list its filter lets through, of how many (the Filter sheet's count); null: not counted. */
  counted: null as { shown: number; total: number } | null,
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
  page === 'playlists'
    ? 'Filter playlists'
    : page === 'library' || page === 'local-albums'
      ? 'Filter albums'
      : page === 'downloads'
        ? 'Filter downloads'
        : page === 'settings'
          ? 'Search settings'
      : page === 'local-artists'
        ? 'Filter artists'
        : page === 'local-tracks'
          ? 'Filter tracks'
          : null;

/** The pages whose view settings the View sheet holds. */
export const hasView = (page: string) => ['playlists', 'library'].includes(page);

/** How many, in words. */
export const howMany = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
