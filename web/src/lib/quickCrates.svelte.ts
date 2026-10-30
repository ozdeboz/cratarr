// Quick crates' state (0.11, lib/quickCrate): whether this server makes them (its capabilities name `quick_crates`,
// lib/capabilities; one before 0.11 has no list: none then), the crate being made at the top of Crates (0.12, the
// owner's mockup B: lib/QuickCrate reads it as a sentence), words handed to it from elsewhere (a search's "Grab a crate"),
// and the last few made here and not saved, kept in this browser to come back to.
import { can } from './capabilities.svelte';
import { CLOSE, type Chip } from './quickCrate';

/** One not saved: its chips and its cut. */
export type KeptQuick = { chips: Chip[]; min: number };

/** How many not saved are kept. */
const KEPT_MOST = 5;
const KEY = 'cratarr-quick-crates';
const CRATES = '#/crates';

/** The server makes them: everything about them shows only then (a server with no list, before 0.11, makes none). */
export const quickOn = () => can('quick_crates') === true;

export const quickCrates = $state({
  /** The crate being made: its chips and its cut (kept while the page is left and come back to). */
  chips: [] as Chip[],
  min: CLOSE,
  /** Words to read into it (a search's), taken by lib/QuickCrate as it shows. */
  words: '',
  recent: readKept(),
});

/** Crates opened on `words` read into a new crate (the one being made, unsaved, kept among the last few first). */
export function openQuickCrate(words = '') {
  if (quickCrates.chips.length) keepQuick({ chips: quickCrates.chips, min: quickCrates.min });
  quickCrates.chips = [];
  quickCrates.min = CLOSE;
  quickCrates.words = words;
  if (!location.hash.startsWith(CRATES)) location.hash = CRATES;
}

/** Crates opened on a crate already read (0.12): a menu's "Sounds like this" or "Grab a crate", global search's crate
 *  card. The one being made is kept among the last few first, as for words. */
export function grabCrate(chips: Chip[]) {
  if (quickCrates.chips.length) keepQuick({ chips: quickCrates.chips, min: quickCrates.min });
  quickCrates.chips = chips.map((c) => ({ ...c }));
  quickCrates.min = CLOSE;
  quickCrates.words = '';
  if (!location.hash.startsWith(CRATES)) location.hash = CRATES;
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
