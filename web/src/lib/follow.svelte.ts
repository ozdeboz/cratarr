// What the page keeps by a playlist's id follows it when it is renamed, deleted or made (and when one of those is
// undone or redone, decision 0026): the pin, the panel's view, Select's ticks, the heart, Insights. Renaming goes through
// here: the server renames it (and its copies), then the page follows the id the server answers with: a playlist's id
// is its own since 0.16 (decision 0101: playlists in the database), so the id stays and there is nothing to follow.
// What the page kept by the keys 0.15 knew playlists by (their names, `owner/name` another's) follows them to their ids
// once (`followMoved`).
import { getMovedPlaylists, renamePlaylist } from './api';
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
  // A pinned playlist stays pinned, in its place; a viewed one stays viewed; its heart, its suggestions' view (its
  // crate `like:list:<id>`) and Insights follow.
  prefs.pinned = prefs.pinned.map((x) => (x === from ? to : x));
  const [was, now] = [`like:list:${from}`, `like:list:${to}`];
  if (prefs.crateViews[was] !== undefined) {
    const { [was]: view, ...rest } = prefs.crateViews;
    prefs.crateViews = { ...rest, [now]: view };
  }
  playlistRenamed(from, to);
  selectRenamed(from, to);
  playlistFavoriteRenamed(from, to);
  insights.renamed(from, to);
}

/** What the page kept by the keys 0.15 knew playlists by follows them to their ids (decision 0101), as the server says
 *  where each leads: on the Playlists page's first showing in a page's life (a key followed once is not kept any more,
 *  so a later showing changes nothing). */
let movedAsked = false;
export async function followMoved() {
  if (movedAsked) return;
  movedAsked = true;
  const moved = await getMovedPlaylists().catch(() => null);
  if (!moved) {
    movedAsked = false;
    return;
  }
  for (const [old, id] of Object.entries(moved)) followRename(old, id);
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
