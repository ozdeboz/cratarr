// Global exclusions (decision 0024, amended 2026-10-19; the owner: "I want crate exclusions and I want global
// exclusions"): a track excluded globally is out of every crate and never picked by Radio, Autoplay or a shuffle of the
// library or the favourites; it still plays when chosen (its album, a playlist, a track tapped). From the full player's
// heart (a long press) and listed from the Library's filter sheet to put back. Undo follows, as a crate's exclusion has.
// A track's menu has it as a switch, "Exclude this track everywhere" (the owner, 2026-10-31), so the ids are kept too.

import { excludedRows, excludeTracks, includeTracks } from './api';
import { loadCrates } from './crates.svelte';
import { excludedIn, withExcluded, type ExcludedIds } from './excludedIds';
import { dropFromQueue } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';

/** How many tracks are excluded globally; null until read. */
export const excluded = $state({ n: null as number | null });

/** Each source's excluded track ids, as read (lib/excludedIds): a source not read yet is absent. */
let ids = $state<ExcludedIds>({});
const asking = new Set<string>();

/** Reads a source's excluded ids (a track's menu, as it opens); `again` after a change elsewhere. Out of reach a list
 *  read before stays as it was. */
export function loadExcludedIds(source: string, again = false) {
  if (asking.has(source) || (source in ids && !again)) return;
  asking.add(source);
  excludedRows(source).then(
    (l) => (ids = { ...ids, [source]: l.rows.flatMap((r) => (r.id ? [r.id] : [])) }),
    () => {},
  ).finally(() => asking.delete(source));
}
/** Whether a track is excluded everywhere; null while its source's list is not read (no switch on a guess). */
export const excludedGloballyOf = (source: string, id: string) => excludedIn(ids, source, id);

/** Reads how many there are (the Library's filter sheet's "Excluded (N)"), and the ids already read again. */
export async function loadExcluded() {
  for (const s of Object.keys(ids)) loadExcludedIds(s, true);
  try {
    const l = await excludedRows(prefs.source || undefined);
    excluded.n = l.excluded ?? l.rows.length;
  } catch {
    excluded.n = null;
  }
}

/** Excludes tracks everywhere; what plays here drops them too, but those added by hand (chosen). */
export async function excludeGlobally(source: string, list: string[]) {
  const { undo, excluded: n } = await excludeTracks(source, list);
  excluded.n = n;
  ids = withExcluded(ids, source, list, true);
  dropFromQueue((t) => t.lane !== 'next' && t.source === source && list.includes(t.id));
  withUndo('Excluded globally.', [undo], {
    after: () => {
      loadExcluded();
      loadCrates();
    },
  });
  loadCrates();
}

/** Takes tracks back everywhere (the switch turned off); Undo excludes them again. */
export async function includeGlobally(source: string, list: string[]) {
  const { undo, excluded: n } = await includeTracks(source, list);
  excluded.n = n;
  ids = withExcluded(ids, source, list, false);
  withUndo('Back everywhere.', [undo], {
    after: () => {
      loadExcluded();
      loadCrates();
    },
  });
  loadCrates();
}

/** A track's switch: out everywhere, or back. */
export function setExcludedGlobally(source: string, id: string, on: boolean) {
  (on ? excludeGlobally : includeGlobally)(source, [id]).catch((e) => toast.fail(e, on ? "It wasn't excluded" : "It wasn't taken back"));
}
