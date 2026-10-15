// The sources the server has connected, for the source picker. Filled by the status call (any page) and by the
// playlist poll (which also says which are playing).
import type { SourceTab } from './api';
import { prefs } from './prefs.svelte';

export const known = $state<{ list: SourceTab[] }>({ list: [] });

/** Cratarr's own source (its own library). No choice means it, where the server has it (the owner, 2026-10-16); only
 *  without it does no choice leave the server to follow whichever started playing most recently. */
export const OWN = 'cratarr';

/** What following no source in particular follows: Cratarr's own where the server has it. */
export const noChoice = () => (known.list.some((s) => s.id === OWN) ? OWN : '');

export function remember(list: SourceTab[]) {
  known.list = list;
  if (!prefs.source) prefs.source = noChoice();
}
