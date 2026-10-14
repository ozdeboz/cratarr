// The hearts on lists' rows (Finamp's, decision 0023): which tracks are favourites, as last asked in one request per
// list, and the long-press that flips one. Shared by the queue sheet and the playlist view.
import { getFavorites, setTrackFavorite } from './api';
import { toast } from './toast.svelte';

/** `source/id` → favourite. */
const favs = $state<Record<string, boolean>>({});
const k = (source: string, id: string) => `${source}/${id}`;

export const isFavorite = (source: string, id: string) => favs[k(source, id)] ?? false;

export function setFavorite(source: string, id: string, on: boolean) {
  favs[k(source, id)] = on;
}

/** Asks which of these tracks are favourites; the hearts stay as they were when the server cannot tell. */
export async function loadFavorites(source: string, ids: string[]) {
  if (!ids.length) return;
  try {
    const found = await getFavorites(source, ids);
    for (const [id, on] of Object.entries(found)) favs[k(source, id)] = on;
  } catch {
    /* as they were */
  }
}

/** The long-press on a row's heart: a favourite or not (a heavy tick, as Finamp's). */
export async function flipFavorite(source: string, id: string) {
  navigator.vibrate?.(25);
  try {
    const s = await setTrackFavorite(source, id, !isFavorite(source, id));
    if (s.favorite !== null) setFavorite(source, id, s.favorite);
  } catch (e) {
    toast.fail(e, "The favourite didn't change");
  }
}
