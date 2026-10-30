// Quick crates' state (0.11, lib/quickCrate): whether this server makes them (its capabilities name `quick_crates`,
// lib/capabilities; one before 0.11 has no list: none then), the sheet (lib/QuickCrateSheet) open with the words it was opened with,
// and the last few made here and not saved, kept in this browser to come back to.
import { can } from './capabilities.svelte';
import { CLOSE, type Chip } from './quickCrate';

/** One not saved: its chips and its cut. */
export type KeptQuick = { chips: Chip[]; min: number };

/** How many not saved are kept. */
const KEPT_MOST = 5;
const KEY = 'cratarr-quick-crates';

/** The server makes them: everything about them shows only then (a server with no list, before 0.11, makes none). */
export const quickOn = () => can('quick_crates') === true;

export const quickCrates = $state({
  /** The sheet shows (App renders it), with these words read into chips at once (a search's, the Crates page's). */
  open: false,
  words: '',
  /** One not saved opened again. */
  from: null as KeptQuick | null,
  recent: readKept(),
});

/** The sheet opened on `words` (none: empty, or the Crates page's field), or on one not saved. */
export function openQuickCrate(words = '', from: KeptQuick | null = null) {
  quickCrates.words = words;
  quickCrates.from = from;
  quickCrates.open = true;
}

function readKept(): KeptQuick[] {
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    return Array.isArray(kept) ? kept.filter((k) => Array.isArray(k?.chips) && k.chips.length).map((k) => ({ chips: k.chips, min: k.min ?? CLOSE })) : [];
  } catch {
    return [];
  }
}
function writeKept() {
  try {
    localStorage.setItem(KEY, JSON.stringify(quickCrates.recent));
  } catch {
    /* not kept */
  }
}
const sameChips = (a: Chip[], b: Chip[]) =>
  a.length === b.length && a.every((c, i) => c.reading.kind === b[i].reading.kind && c.reading.value === b[i].reading.value && c.how === b[i].how && !!c.own === !!b[i].own);

/** One left unsaved: first among the kept, once. */
export function keepQuick(q: KeptQuick) {
  if (!q.chips.length) return;
  quickCrates.recent = [$state.snapshot(q) as KeptQuick, ...quickCrates.recent.filter((k) => !sameChips(k.chips, q.chips))].slice(0, KEPT_MOST);
  writeKept();
}
/** One saved (or removed): no longer kept. */
export function forgetQuick(chips: Chip[]) {
  quickCrates.recent = quickCrates.recent.filter((k) => !sameChips(k.chips, chips));
  writeKept();
}
