// The sources the server has connected, for the filter's server cards (lib/FilterSheet: which server a browser follows
// filters what it shows, decided 2026-10-16; no dropdown on the pages). Filled by the shell's status call and by the
// playlist poll (which also says which are playing).
import type { SourceTab } from './api';
import { prefs } from './prefs.svelte';

export const known = $state<{ list: SourceTab[] }>({ list: [] });

/** Cratarr's own source (its own library). No choice means it, where the server has it (the owner, 2026-10-16); only
 *  without it does no choice leave the server to follow whichever started playing most recently. */
export const OWN = 'cratarr';

/** What following no source in particular follows: Cratarr's own where the server has it. */
export const noChoice = () => (known.list.some((s) => s.id === OWN) ? OWN : '');

/** Whether a server's library shows on the album pages (the filter's library switches, remembered). */
export const libraryShown = (source: string, library: string) => !prefs.hiddenLibraries.includes(`${source}/${library}`);

/** Switches a library on or off. */
export function showLibrary(source: string, library: string, on: boolean) {
  const key = `${source}/${library}`;
  prefs.hiddenLibraries = on ? prefs.hiddenLibraries.filter((k) => k !== key) : [...prefs.hiddenLibraries, key];
}

export function remember(list: SourceTab[]) {
  known.list = list;
  // No choice, or a choice the server no longer has (a source switched off: it would list nothing), is Cratarr's own.
  if (!list.some((s) => s.id === prefs.source)) prefs.source = noChoice();
}
