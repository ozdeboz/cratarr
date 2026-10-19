// The hearts on lists' rows (Finamp's, decision 0023): which tracks are favourites, as last asked in one request per
// list, and the long-press that flips one. Shared by the queue sheet and the playlist view.
import { getFavoriteAlbums, getFavorites, getPlaylistFavorites, setPlaylistFavorite, setTrackFavorite } from './api';
import { prefs } from './prefs.svelte';
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

// Albums' hearts: a server's whole list at once (an album's star, the one Subsonic apps give it), in the same map as
// the tracks' (an album's id is not a track's). The last list per server, so an album no longer on it loses its heart.
const albumLists: Record<string, Set<string>> = {};
const albumAsks: Record<string, Promise<boolean>> = {};

/** Asks for a server's favourite albums (one request at a time per server); `ids`: albums to say about either way.
 *  False when the server cannot tell; the hearts stay as they were. */
export function loadAlbumFavorites(source: string, ids: string[] = []): Promise<boolean> {
  // The device's own music (local mode, decision 0052) has no hearts.
  if (source === 'device') return Promise.resolve(false);
  albumAsks[source] ??= getFavoriteAlbums(source)
    .then((list) => {
      const now = new Set(list);
      for (const id of albumLists[source] ?? []) if (!now.has(id)) favs[k(source, id)] = false;
      for (const id of now) favs[k(source, id)] = true;
      albumLists[source] = now;
      return true;
    })
    .catch(() => false)
    .finally(() => delete albumAsks[source]);
  return albumAsks[source].then((ok) => {
    if (ok) for (const id of ids) favs[k(source, id)] = albumLists[source].has(id);
    return ok;
  });
}

// Playlists' hearts (Finamp's, decision 0023): which playlists are favourites, asked for once; a change shows at once.
const playlistFavs = $state<Record<string, boolean>>({});
let playlistFavsAsked = false;

export const isPlaylistFavorite = (name: string) => playlistFavs[name] ?? false;

export async function loadPlaylistFavorites() {
  if (playlistFavsAsked) return;
  playlistFavsAsked = true;
  try {
    Object.assign(playlistFavs, await getPlaylistFavorites(prefs.source || undefined));
  } catch {
    playlistFavsAsked = false;
  }
}

/** Makes a playlist a favourite or not (every copy the server has of it). */
export async function togglePlaylistFavorite(name: string) {
  const on = !isPlaylistFavorite(name);
  playlistFavs[name] = on;
  navigator.vibrate?.(8);
  try {
    await setPlaylistFavorite(name, on, prefs.source || undefined);
  } catch (e) {
    playlistFavs[name] = !on;
    toast.fail(e, "The favourite didn't change");
  }
}

/** Several playlists at once (the selection's ⋮): all favourites, or none. */
export async function setPlaylistFavorites(names: string[], on: boolean) {
  const was = names.map((n) => [n, isPlaylistFavorite(n)] as const);
  for (const n of names) playlistFavs[n] = on;
  navigator.vibrate?.(8);
  try {
    for (const [n, before] of was) if (before !== on) await setPlaylistFavorite(n, on, prefs.source || undefined);
  } catch (e) {
    for (const [n, before] of was) playlistFavs[n] = before;
    toast.fail(e, "The favourites didn't change");
  }
}

/** A renamed playlist keeps its heart (the server's copies follow the new name). */
export function playlistFavoriteRenamed(from: string, to: string) {
  if (!(from in playlistFavs)) return;
  playlistFavs[to] = playlistFavs[from];
  delete playlistFavs[from];
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
