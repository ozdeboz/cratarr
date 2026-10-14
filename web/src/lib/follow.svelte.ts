// What the page keeps by a playlist's name follows it when it is renamed, deleted or made (and when one of those is
// undone or redone, decision 0026): the pin, the panel's view, Select's ticks, the heart, Insights. Renaming goes through
// here: the server renames it (its file and its copies), then the page follows.
import { renamePlaylist } from './api';
import { playlistFavoriteRenamed } from './favourites.svelte';
import { insights } from './insights.svelte';
import { playlistDeleted, playlistRenamed } from './panel.svelte';
import { selectDeleted, selectRenamed } from './select.svelte';
import { prefs } from './prefs.svelte';

/** Returns the rename's undo token (decision 0026). */
export async function renameEverywhere(from: string, to: string) {
  const { undo } = await renamePlaylist(from, to);
  followRename(from, to);
  return undo;
}

/** What the page keeps by a playlist's name follows a rename (also its undo and redo). */
export function followRename(from: string, to: string) {
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
