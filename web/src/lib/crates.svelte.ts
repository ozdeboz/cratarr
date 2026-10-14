// The permanent crates and their sizes (decision 0024), for the Crates page and the crate open in the panel. Asked for
// when looked at, and again when the playlists change (sorting a track into one shrinks Unlisted).
import { getCrates, type Crate } from './api';
import { prefs } from './prefs.svelte';

export const crates = $state<{ list: Crate[] | null; error: string | null }>({ list: null, error: null });

export async function loadCrates() {
  try {
    crates.list = await getCrates(prefs.source || undefined);
    crates.error = null;
  } catch (e) {
    crates.error = e instanceof Error ? e.message : 'Unexpected error';
  }
}

/** A crate's id in the panel's address (`?open=crate:unlisted`), and back. */
export const CRATE = 'crate:';
export const crateOf = (open: string | null) => (open?.startsWith(CRATE) ? open.slice(CRATE.length) : null);
export const crateName = (id: string) => crates.list?.find((c) => c.id === id)?.name ?? id;
