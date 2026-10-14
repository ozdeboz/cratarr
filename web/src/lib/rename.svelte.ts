// Renaming a playlist, from its menu or its view's edit mode: the server renames it (its file and its copies), and what
// the page keeps by its name follows: the pin, the open panel, Select's ticks, the heart, Insights.
import { renamePlaylist } from './api';
import { playlistFavoriteRenamed } from './favourites.svelte';
import { insights } from './insights.svelte';
import { playlistRenamed } from './panel.svelte';
import { selectRenamed } from './select.svelte';
import { prefs } from './prefs.svelte';

export async function renameEverywhere(from: string, to: string) {
  await renamePlaylist(from, to);
  // A pinned playlist stays pinned, in its place; an open one stays open; its heart and Insights follow.
  prefs.pinned = prefs.pinned.map((x) => (x === from ? to : x));
  playlistRenamed(from, to);
  selectRenamed(from, to);
  playlistFavoriteRenamed(from, to);
  insights.renamed(from, to);
}
