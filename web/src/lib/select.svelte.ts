// Select on the Playlists page (decision 0025): a tap on a chip's name ticks the playlist into the selection instead of
// viewing it alone; the chip's circle keeps adding or removing the playing track. Beside the page the panel shows the
// selection as it is ticked; on a phone (the panel covers the page) it is ticked first and viewed with "View N".
import { sidePanel } from './live.svelte';
import { closePlaylist, viewSelection, panel } from './panel.svelte';
import { leaveOnArrival } from './navStep.svelte';

export const select = $state({ on: false, picked: [] as string[] });

/** Starts Select with what is selected already (the playlist viewed in the panel, or the selection). */
export function startSelect() {
  select.on = true;
  select.picked = [...panel.selection];
}

/** Select ends (a tap on it again): a panel showing the selection keeps only the playlist ticked last; a closed panel
 *  stays closed. */
export function endSelect() {
  select.on = false;
  select.picked = [];
  const last = panel.selection.at(-1);
  if (last && panel.selection.length > 1) viewSelection([last], panel.tab);
}

/** The panel shows exactly what is ticked. */
export const showsPicked = () => select.picked.length > 0 && select.picked.join('\n') === panel.selection.join('\n');

/** Ticks or unticks a playlist; beside the page the panel follows (on the tab it shows). */
export function tick(id: string) {
  select.picked = select.picked.includes(id) ? select.picked.filter((x) => x !== id) : [...select.picked, id];
  if (sidePanel()) viewPicked();
}

/** The select sheet's choice (lib/SelectSheet) among these playlists (all shown): all, none, or the others. */
export function pickWhich(ids: string[], which: 'all' | 'none' | 'others') {
  select.picked = which === 'all' ? [...ids] : which === 'none' ? [] : ids.filter((id) => !select.picked.includes(id));
  if (sidePanel()) viewPicked();
}

/** Views what is ticked in the panel (a phone's "View N playlists"). */
export function viewPicked() {
  if (!select.picked.length) return void (panel.selection.length && closePlaylist());
  viewSelection(select.picked, panel.selection.length ? panel.tab : 'tracks');
}

/** Leaves one playlist out of the selection (its ✕ in the Export tab): the panel and the ticks follow. */
export function leaveOut(id: string) {
  select.picked = select.picked.filter((x) => x !== id);
  viewSelection(
    panel.selection.filter((x) => x !== id),
    panel.tab,
  );
}

const onPlaylists = () => (location.hash.split('?')[0] || '#/') === '#/';

/** Add playlists (the full-screen panel's Export tab): Select on the Playlists page, with what is selected; the panel
 *  closes so the chips show. (Beside the page they show already: ticking them adds.) */
export function addMore() {
  if (!select.on) startSelect();
  // Its layer stays over the page until the Playlists page shows (lib/navStep).
  if (!onPlaylists()) leaveOnArrival({ layer: true }, false, '#/');
  closePlaylist(() => !onPlaylists() && (location.hash = '#/'));
}

/** A renamed playlist stays ticked; a deleted one is left out. */
export function selectRenamed(from: string, to: string) {
  select.picked = select.picked.map((x) => (x === from ? to : x));
}
export function selectDeleted(id: string) {
  select.picked = select.picked.filter((x) => x !== id);
}
