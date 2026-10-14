// Select on the Playlists page (decision 0025): a tap on a chip's name ticks the playlist into the selection instead of
// opening it alone; the chip's circle keeps adding or removing the playing track. Beside the page the panel shows the
// selection as it is ticked; on a phone (the panel covers the page) it is ticked first and opened with "Open N".
import { sidePanel } from './live.svelte';
import { closePlaylist, keepOpen, openSelection, panel } from './panel.svelte';

export const select = $state({ on: false, picked: [] as string[] });

/** Starts Select with what is selected already (the playlist open in the panel, or the selection). */
export function startSelect() {
  select.on = true;
  select.picked = [...panel.selection];
}

/** Cancel: Select ends; a selection of several in the panel closes with it (one playlist stays open). */
export function endSelect() {
  select.on = false;
  select.picked = [];
  if (panel.selection.length > 1) closePlaylist();
}

/** The panel shows exactly what is ticked. */
export const showsPicked = () => select.picked.length > 0 && select.picked.join('\n') === panel.selection.join('\n');

/** Ticks or unticks a playlist; beside the page the panel follows (on the tab it shows). */
export function tick(id: string) {
  select.picked = select.picked.includes(id) ? select.picked.filter((x) => x !== id) : [...select.picked, id];
  if (sidePanel()) openPicked();
}

/** Opens what is ticked in the panel (a phone's "Open N playlists"). */
export function openPicked() {
  if (!select.picked.length) return void (panel.selection.length && closePlaylist());
  openSelection(select.picked, panel.selection.length ? panel.tab : 'tracks');
}

/** Leaves one playlist out of the selection (its ✕ in the Export tab): the panel and the ticks follow. */
export function leaveOut(id: string) {
  select.picked = select.picked.filter((x) => x !== id);
  openSelection(
    panel.selection.filter((x) => x !== id),
    panel.tab,
  );
}

const onPlaylists = () => (location.hash.split('?')[0] || '#/') === '#/';
/** Select is on the Playlists page beside this panel: ticks there show here at once. */
export const tickingBeside = () => select.on && sidePanel() && onPlaylists();

/** Add playlists (the Export tab): Select on the Playlists page, with what is selected. Beside the page the panel
 *  stays; on a phone it closes, so the chips show. */
export function addMore() {
  if (!select.on) startSelect();
  if (sidePanel()) {
    if (!onPlaylists()) location.hash = keepOpen('#/');
    return;
  }
  closePlaylist(() => !onPlaylists() && (location.hash = '#/'));
}

/** A renamed playlist stays ticked; a deleted one is left out. */
export function selectRenamed(from: string, to: string) {
  select.picked = select.picked.map((x) => (x === from ? to : x));
}
export function selectDeleted(id: string) {
  select.picked = select.picked.filter((x) => x !== id);
}
