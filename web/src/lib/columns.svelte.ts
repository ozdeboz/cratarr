// Columns where there is room (decision 0042): each kind of list has its columns in the owner's order (View), and a
// list shows as many from the top of it as its own width allows, each at least its own width; the rest stay hidden. A
// list measures itself, not the window: a panel beside the page and one over it differ.

import { prefs } from './prefs.svelte';

export type ColumnKind = 'tracks' | 'albums' | 'playlists' | 'artists';
export type Column = { id: string; label: string; width: number; end?: boolean; grow?: boolean };

/** A kind's columns in their first order; `width` is the least each needs (px). A number is set to the end (`end`); a
 *  name takes a share of the room a row has left (`grow`); the rest keep their width. */
export const COLUMNS: Record<ColumnKind, readonly Column[]> = {
  tracks: [
    { id: 'album', label: 'Album', width: 200, grow: true },
    { id: 'tempo', label: 'Tempo', width: 56, end: true },
    { id: 'plays', label: 'Plays', width: 52, end: true },
    { id: 'lastPlayed', label: 'Last played', width: 96 },
    { id: 'added', label: 'Added', width: 96 },
    { id: 'year', label: 'Year', width: 48, end: true },
    { id: 'albumArtist', label: 'Album artist', width: 160, grow: true },
  ],
  // The Library's and Downloads' Table (lib/AlbumShelf); its length from 0.10's server (the owner, 2026-10-30).
  albums: [
    { id: 'year', label: 'Year', width: 48, end: true },
    { id: 'tracks', label: 'Tracks', width: 52, end: true },
    { id: 'length', label: 'Length', width: 64, end: true },
    { id: 'added', label: 'Added', width: 96 },
    { id: 'plays', label: 'Plays', width: 52, end: true },
    { id: 'lastPlayed', label: 'Last played', width: 96 },
    { id: 'format', label: 'Format', width: 120 },
    { id: 'rating', label: 'Rating', width: 76 },
  ],
  // The Library's Artists (the owner, 2026-10-25: the same table as its albums').
  artists: [
    { id: 'albums', label: 'Albums', width: 56, end: true },
    { id: 'tracks', label: 'Tracks', width: 56, end: true },
  ],
  // The Playlists page's Table; how many tracks stays at a line's end, as a track's length does.
  playlists: [
    { id: 'length', label: 'Length', width: 72, end: true },
    { id: 'genres', label: 'Genres', width: 180, grow: true },
    { id: 'lastPlayed', label: 'Last played', width: 96 },
    { id: 'changed', label: 'Changed', width: 96 },
    { id: 'unplayed', label: 'Unplayed', width: 72, end: true },
  ],
};

/** What a row keeps for itself before any column: its cover, its title at a readable width, its time and its menu. */
/** The Library's Tracks' columns: the ones its server lists give values for (/api/library/tracks). */
export const LIBRARY_TRACK_COLUMNS: readonly string[] = ['album', 'tempo', 'plays', 'lastPlayed', 'added', 'year', 'albumArtist'];
const OWN: Record<ColumnKind, number> = { tracks: 60 + 280 + 52 + 40 + 40, albums: 48 + 280 + 40 + 30, playlists: 240 + 52 + 40 + 30, artists: 48 + 240 + 30 };
/** Between two columns. */
export const COLUMN_GAP = 16;

/** A kind's columns in the owner's order; a column they have not placed yet comes last. */
export function ordered(kind: ColumnKind): Column[] {
  const all = COLUMNS[kind];
  const order = prefs.columns[kind] ?? [];
  return [...order.flatMap((id) => all.filter((c) => c.id === id)), ...all.filter((c) => !order.includes(c.id))];
}

/** The columns a list of this width shows: from the top of the order, as long as the next one fits. `has`: whether the
 *  list knows a column's values (a server's playlist has no plays). */
export function fit(kind: ColumnKind, width: number, has: (id: string) => boolean = () => true): Column[] {
  let room = width - OWN[kind];
  const out: Column[] = [];
  for (const c of ordered(kind).filter((c) => has(c.id))) {
    if (room < c.width + COLUMN_GAP) break;
    room -= c.width + COLUMN_GAP;
    out.push(c);
  }
  return out;
}

/** The owner's order for a kind (dragged in View). */
export function setOrder(kind: ColumnKind, ids: string[]) {
  prefs.columns = { ...prefs.columns, [kind]: ids };
}
