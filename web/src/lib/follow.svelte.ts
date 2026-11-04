// What the page keeps by a playlist's id follows it when it is renamed, deleted or made (and when one of those is
// undone or redone, decision 0026): the pin, the panel's view, Select's ticks, the heart, Insights. Renaming goes through
// here: the server renames it (its file and its copies), then the page follows the id the server answers with. A
// playlist's id is its name today (its owner's too for another's shared one), so a rename moves it; with 0.16's ids
// (decision 0101: playlists in the database) the id stays and there is nothing to follow.
import { renamePlaylist } from './api';
import { playlistFavoriteRenamed } from './favourites.svelte';
import { insights } from './insights.svelte';
import { playlistDeleted, playlistRenamed } from './panel.svelte';
import { selectDeleted, selectRenamed } from './select.svelte';
import { prefs } from './prefs.svelte';

/** Renames playlist `from` to the name `to`: the rename's undo token (decision 0026) and the id it has now. */
export async function renameEverywhere(from: string, to: string): Promise<{ undo: string | null; id: string }> {
  const { undo, playlist } = await renamePlaylist(from, to);
  followRename(from, playlist);
  return { undo, id: playlist };
}

/** What the page keeps by a playlist's id follows it from `from` to `to` (a rename, its undo and redo); nothing where
 *  the id stayed. */
export function followRename(from: string, to: string) {
  if (from === to) return;
  // A pinned playlist stays pinned, in its place; a viewed one stays viewed; its heart and Insights follow.
  prefs.pinned = prefs.pinned.map((x) => (x === from ? to : x));
  playlistRenamed(from, to);
  selectRenamed(from, to);
  playlistFavoriteRenamed(from, to);
  insights.renamed(from, to);
}

/** A playlist went (deleted, or a new one's undo): the panel and Select let it go, Insights leaves it out. */
export function followDelete(id: string) {
  playlistDeleted(id);
  selectDeleted(id);
  insights.deleted(id);
}

/** A playlist came (back): Insights shows it again. */
export function followCreate(id: string) {
  insights.restored(id);
}
