// A part's own search (the owner, 2026-10-25: the search on every panel, Settings' too): over the page on a phone, the
// Settings panel's search leaves out the rows (lib/settings/Item) whose name lacks what is searched, and a group with
// none left; a group whose title has it keeps all its rows. Beside the page, the page's search is the one (decision
// 0072), and the panel shows all.
import { getContext, setContext } from 'svelte';

const NARROW = Symbol('settings-narrow');
const GROUP_HIT = Symbol('settings-group-hit');

/** The panel's: whether its rows are narrowed by the search now. */
export const narrowSettings = (on: () => boolean) => setContext(NARROW, on);
/** A row's or a group's: whether the part is narrowed now. */
export function settingsNarrowed(): () => boolean {
  const on = getContext<(() => boolean) | undefined>(NARROW);
  return () => on?.() ?? false;
}
/** A group's: whether its title holds what is searched (its rows all stay). */
export const groupHit = (hit: () => boolean) => setContext(GROUP_HIT, hit);
export function inGroupHit(): () => boolean {
  const hit = getContext<(() => boolean) | undefined>(GROUP_HIT);
  return () => hit?.() ?? false;
}
