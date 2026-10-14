// The permanent crates and their sizes (decision 0024), for the Crates page and the crate open in the panel. Asked for
// when looked at, and again when the playlists change (sorting a track into one shrinks Unlisted).
import { getCrates, type Crate, type CrateFavourites } from './api';
import { prefs } from './prefs.svelte';
import { openPlaylist } from './panel.svelte';

export const crates = $state<{ list: Crate[] | null; error: string | null }>({ list: null, error: null });

export async function loadCrates() {
  try {
    crates.list = await getCrates(prefs.source || undefined);
    crates.error = null;
  } catch (e) {
    crates.error = e instanceof Error ? e.message : 'Unexpected error';
  }
}

// The favourites toggle, per crate, remembered by this browser: a crate not set yet shows no favourites.
const FAV_KEY = 'cratarr-crate-favourites';
const favs = $state<Record<string, CrateFavourites>>(
  (() => {
    try {
      const k = JSON.parse(localStorage.getItem(FAV_KEY) ?? 'null');
      return k && typeof k === 'object' ? k : {};
    } catch {
      return {};
    }
  })(),
);
export const favouritesOf = (id: string): CrateFavourites => favs[id] ?? 'none';
const NEXT: Record<CrateFavourites, CrateFavourites> = { none: 'only', only: 'all', all: 'none' };
/** A tap on the toggle: No favourites, Only favourites, All tracks, and round again. */
export function cycleFavourites(id: string) {
  favs[id] = NEXT[favouritesOf(id)];
  try {
    localStorage.setItem(FAV_KEY, JSON.stringify(favs));
  } catch {
    /* not kept */
  }
}

/** A crate's id in the panel's address (`?open=crate:unlisted`), and back. */
export const CRATE = 'crate:';
export const crateOf = (open: string | null) => (open?.startsWith(CRATE) ? open.slice(CRATE.length) : null);

/** The builder (decision 0024, the panel's third mode): `?open=crate-new`, or `?open=crate-edit:<id>` for one's own crate.
 *  `{ id: null }` is a new crate. */
export const NEW_CRATE = 'crate-new';
export const EDIT_CRATE = 'crate-edit:';
export const builderOf = (open: string | null): { id: string | null } | null =>
  open === NEW_CRATE ? { id: null } : open?.startsWith(EDIT_CRATE) ? { id: open.slice(EDIT_CRATE.length) } : null;
export const crateName = (id: string) => crates.list?.find((c) => c.id === id)?.name ?? id;

/** A new crate, named in the name sheet (as a new playlist is): the builder opens on a fresh draft of that name, saved
 *  only with its Save (a crate without conditions would hold the whole library). */
export function startCrate(name: string) {
  try {
    localStorage.setItem(`cratarr-crate-draft:new`, JSON.stringify({ name, conditions: [] }));
  } catch {
    /* the builder starts empty */
  }
  openPlaylist(NEW_CRATE);
}
