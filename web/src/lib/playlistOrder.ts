// The playlists' orders, pure so the tests take them as they are: the Playlists page's (its header, View, its table's
// column names) and a menu's Add to playlist's. Length and Last played (the owner, 2026-10-31) come from the playlists'
// facts (GET /api/playlists/stats), read after the list: one without the fact goes last whichever way round, its order
// among the others by name. Playing first left the page (the owner, 2026-10-31: it said nothing there); a menu keeps it
// as Ticked first, those holding the item first.
import type { PlaylistOrder } from './prefs.svelte';

type OrderField<K extends string> = { value: K; label: string; most?: true };

export const PLAYLIST_ORDERS: OrderField<PlaylistOrder>[] = [
  { value: 'name', label: 'Name' },
  { value: 'tracks', label: 'Tracks' },
  { value: 'length', label: 'Length', most: true },
  { value: 'lastPlayed', label: 'Last played', most: true },
  { value: 'missing', label: 'Missing', most: true },
];

/** A menu's Add to playlist (lib/PlaylistToggles): by name, by size, or those it holds first. */
export const LIST_ORDERS: OrderField<PlaylistOrder>[] = [
  { value: 'name', label: 'Name' },
  { value: 'tracks', label: 'Tracks' },
  { value: 'playing', label: 'Ticked first' },
];

/** The orders that go by a playlist's facts, read after the list. */
export const BY_FACTS: readonly PlaylistOrder[] = ['length', 'lastPlayed'];

/** What the facts say of a playlist (its stats), as far as an order needs them. */
export type PlaylistFacts = { seconds: number; last_played: number | null };

/** The playlists' order, either way round (`down`): by name, by size, by missing entries, those holding the item first
 *  (`holds`), or by a fact (`factsOf`), one without it last both ways; equals by name, turned round with the rest. */
export function playlistBy<T extends { id: string; name: string; count: number; dead?: number }>(
  order: PlaylistOrder,
  down: boolean,
  holds: (p: T) => boolean,
  factsOf: (p: T) => PlaylistFacts | undefined = () => undefined,
) {
  const way = down ? -1 : 1;
  const byName = (a: T, b: T) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true });
  const fact =
    order === 'length' ? (p: T) => factsOf(p)?.seconds || null
    : order === 'lastPlayed' ? (p: T) => factsOf(p)?.last_played ?? null
    : null;
  return (a: T, b: T) => {
    if (fact) {
      const [x, y] = [fact(a), fact(b)];
      if ((x === null) !== (y === null)) return x === null ? 1 : -1;
      if (x !== null && y !== null && x !== y) return way * (x - y);
      return way * byName(a, b);
    }
    return (
      way *
      (order === 'tracks' ? a.count - b.count || byName(a, b)
      : order === 'missing' ? (a.dead ?? 0) - (b.dead ?? 0) || byName(a, b)
      : order === 'playing' ? Number(holds(b)) - Number(holds(a)) || byName(a, b)
      : byName(a, b))
    );
  };
}

/** The Playlists table's column names that order it (lib/columnHead): Name and Tracks are the row's own. */
export const PLAYLIST_ORDER_OF: Partial<Record<string, PlaylistOrder>> = { length: 'length', lastPlayed: 'lastPlayed', missing: 'missing' };
