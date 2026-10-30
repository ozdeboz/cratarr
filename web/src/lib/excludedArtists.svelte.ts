// Artists left out (0.10, decision 0083; the owner, 2026-10-30): everywhere (every crate, the fixed ones too, radio,
// Autoplay, Discover), the account's one list; or of one crate ("Not in this crate", an Artist-is-not row of its filter).
// Read when a menu or Settings needs it; each change says so with Undo. A server before 0.10 has neither: nothing shows.
import { backInCrate, excludeArtist, getCrate, getExcludedArtists, includeArtist, notInCrate, type ExcludedArtist } from './api';
import { artistKey } from './artistCredits';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';

export const excludedArtists = $state({
  /** The account's, everywhere; null until read (or a server without them). */
  everywhere: null as ExcludedArtist[] | null,
  /** Each crate's own, by crate id, as read. */
  crates: {} as Record<string, ExcludedArtist[]>,
  /** Goes up when a crate's own change: its panel reads its rows again. */
  version: 0,
});

let asked = false;
/** Read once (again after a change elsewhere); a server without the list leaves it null. */
export function loadExcludedArtists(again = false) {
  if (asked && !again) return;
  asked = true;
  getExcludedArtists().then(
    (l) => (excludedArtists.everywhere = l ?? []),
    () => {},
  );
}
export const excludedEverywhere = (name: string) => !!excludedArtists.everywhere?.some((a) => a.key === artistKey(name) || artistKey(a.name) === artistKey(name));

/** Out everywhere, or back. */
export async function setExcludedEverywhere(name: string, on: boolean) {
  const key = artistKey(name);
  try {
    if (on) {
      const r = await excludeArtist(key, name);
      excludedArtists.everywhere = [...(excludedArtists.everywhere ?? []).filter((a) => a.key !== r.key), { key: r.key, name: r.name, at: r.at }];
      withUndo(`${name} excluded everywhere.`, [r.undo], { after: () => loadExcludedArtists(true) });
    } else {
      const was = excludedArtists.everywhere?.find((a) => a.key === key || artistKey(a.name) === key);
      const r = await includeArtist(was?.key ?? key);
      excludedArtists.everywhere = (excludedArtists.everywhere ?? []).filter((a) => a.key !== r.key);
      withUndo(`${name} back everywhere.`, [r.undo], { after: () => loadExcludedArtists(true) });
    }
  } catch (e) {
    toast.fail(e, on ? `Could not exclude ${name}` : `Could not take ${name} back`);
  }
}

/** A crate's own, read when its panel shows (a server before 0.10: none). */
export function loadCrateArtists(crate: string) {
  getCrate(crate).then(
    (c) => (excludedArtists.crates[crate] = c?.not_artists ?? []),
    () => {},
  );
}
export const notInThisCrate = (crate: string | null, name: string) => !!crate && !!excludedArtists.crates[crate]?.some((a) => a.key === artistKey(name) || artistKey(a.name) === artistKey(name));

/** Out of one crate, or back in it. */
export async function setNotInCrate(crate: string, name: string, on: boolean) {
  try {
    const was = excludedArtists.crates[crate]?.find((a) => a.key === artistKey(name) || artistKey(a.name) === artistKey(name));
    const r = on ? await notInCrate(crate, name) : await backInCrate(crate, was?.key ?? artistKey(name));
    excludedArtists.crates[crate] = r.not_artists;
    excludedArtists.version++;
    withUndo(on ? `${name} out of this crate.` : `${name} back in this crate.`, [r.undo], { after: () => (loadCrateArtists(crate), excludedArtists.version++) });
  } catch (e) {
    toast.fail(e, on ? `Could not leave ${name} out` : `Could not take ${name} back`);
  }
}
