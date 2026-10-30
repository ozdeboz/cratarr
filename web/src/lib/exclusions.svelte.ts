// Global exclusions (decision 0024, amended 2026-10-19; the owner: "I want crate exclusions and I want global
// exclusions"): a track excluded globally is out of every crate and never picked by Radio, Autoplay or a shuffle of the
// library or the favourites; it still plays when chosen (its album, a playlist, a track tapped). From the full player's
// heart (a long press) and listed from the Library's filter sheet to put back. Undo follows, as a crate's exclusion has.

import { excludedRows, excludeTracks } from './api';
import { loadCrates } from './crates.svelte';
import { dropFromQueue } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';
import { withUndo } from './undo.svelte';

/** How many tracks are excluded globally; null until read. */
export const excluded = $state({ n: null as number | null });

/** Reads how many there are (the Library's filter sheet's "Excluded (N)"). */
export async function loadExcluded() {
  try {
    const l = await excludedRows(prefs.source || undefined);
    excluded.n = l.excluded ?? l.rows.length;
  } catch {
    excluded.n = null;
  }
}

/** Excludes tracks everywhere; what plays here drops them too, but those added by hand (chosen). */
export async function excludeGlobally(source: string, ids: string[]) {
  const { undo, excluded: n } = await excludeTracks(source, ids);
  excluded.n = n;
  dropFromQueue((t) => t.lane !== 'next' && t.source === source && ids.includes(t.id));
  withUndo('Excluded globally.', [undo], {
    after: () => {
      loadExcluded();
      loadCrates();
    },
  });
  loadCrates();
}
