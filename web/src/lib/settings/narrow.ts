// A part's own search (the owner, 2026-10-25: the search on every panel, Settings' too): over the page on a phone, the
// Settings panel's search leaves out the rows (lib/settings/Item) whose name lacks what is searched, and a group with
// none left; a group whose title has it keeps all its rows. Beside the page, the page's search is the one (decision
// 0072), and the panel shows all.
import { getContext, setContext } from 'svelte';
import type { SettingsSection } from '../settingsSections.svelte';
import { basicShows, groupTier } from './tiers';

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

// Basic (CU23): the part whose Basic rows alone show now, from the panel (lib/SettingsPanel): null where it shows whole (Advanced,
// a thing's page, a part opened by its address, a part Basic does not touch). A row hides through it as it does for the search
// (lib/settings/Item, its `left`), and a group with every row hidden, or one whose title is Advanced, goes too.
const BASIC = Symbol('settings-basic');

/** The panel's: which part's Basic applies now, or null. */
export const basicSettings = (part: () => SettingsSection | null) => setContext(BASIC, part);
/** A row's: whether Basic leaves a row of this name out now. */
export function basicLeaves(): (name: string) => boolean {
  const part = getContext<(() => SettingsSection | null) | undefined>(BASIC);
  return (name) => {
    const p = part?.() ?? null;
    return p !== null && !basicShows(p, name);
  };
}
/** A group's: whether Basic applies now, and whether it leaves out a group of this title whole. */
export function basicGroups(): { on: () => boolean; out: (title: string | null) => boolean } {
  const part = getContext<(() => SettingsSection | null) | undefined>(BASIC);
  return {
    on: () => (part?.() ?? null) !== null,
    out: (title) => {
      const p = part?.() ?? null;
      return p !== null && groupTier(p, title) === 'advanced';
    },
  };
}
