// Basic and Advanced, each row's tier declared once (CU21; the owner, 2026-11-06: one global toggle for the settings, "Basic"
// a row a person must answer or changes often; 2026-11-17: one declared home, read by the code and checked by a test). Read
// again at trunk from the parts themselves, with the settings' index of 2026-11-06 and the owner's calls on it (2026-11-06; and
// 2026-11-16: plus Mobile data) saying which are Basic; every other row is Advanced. Only Personal's and
// This device's parts have tiers: an administrator's settings get no Basic variation. Pure, so the tests take it as it is
// (tests/settings-tiers.test.ts: every row a tier, every tier a row).
//
// A row is named as it shows (its `name`); a row of a thing (a device, a sign-in, a library's folder) has its thing's name and
// no tier: it shows wherever its group does. A group whose rows are things goes by its title. A thing's own page (a server, a
// listening service, Spotify) always shows whole: it is opened from a row that shows.
import type { SettingsSection } from '../settingsSections.svelte';

export type Tier = 'basic' | 'advanced';

/** The parts Basic touches: Personal's and This device's. */
export const TIERED: readonly SettingsSection[] = ['account', 'taste', 'radio', 'tracking', 'import', 'device', 'network', 'sync', 'appearance', 'layout', 'player', 'content'];

/** Parts all of whose rows are Advanced: in Basic they are not listed (opened by their address, they show whole). */
export const ADVANCED_PARTS: readonly SettingsSection[] = ['radio', 'import', 'layout', 'player', 'content'];

const B: Tier = 'basic';
const A: Tier = 'advanced';

/** The other tiered parts' rows, by the name each shows (a Reset's item by the name it resets, where no row has it). */
export const ROWS: Partial<Record<SettingsSection, Readonly<Record<string, Tier>>>> = {
  account: { Password: B, 'App password': A, 'Sign out': B, Spotify: B, 'Allow a device': B, 'Pair an app': B },
  taste: { Analysed: A, 'Your profile': A, Everywhere: B, Artists: B, Discover: B },
  tracking: { 'Keep play history': B, 'Delete play history': B, 'Scrobble from': A, 'User token': A, 'API key': A, 'Shared secret': A },
  device: {
    'Player name': A,
    'Where music plays': B,
    'Level the volume': B,
    'Not measured yet': A,
    Waveform: A,
    Tape: A,
    'Snippets in menus': A,
    Lossless: A,
    'Stay ready': A,
    Background: A,
    'Library on this device': A,
    'Weather for the moment crate': A,
    "This device's music": A,
    'No music found on this device': A,
    'Mode switch in the bar': A,
    Effect: A,
    Start: A,
    Stop: A,
    'Also for next and previous': A,
    'Tape winding': A,
    'Track boundary': A,
    Animation: A,
    'Open by themselves': A,
    After: A,
    'Start from': A,
    'The look': A,
    Cratarr: A,
    'Add a server': A,
  },
  network: {
    'Cratarr does not answer': B,
    'Public address': B,
    'Prefer local network': A,
    'Local network address': A,
    'Active address': A,
    'Test both connections': A,
  },
  sync: { Storage: A, Quality: A, 'Mobile data': B, 'Only while charging': A, 'Kept in this browser': B },
  appearance: {
    Theme: B,
    Automatic: B,
    Animations: A,
    'Personal greeting': A,
    'Control glass': A,
    'Mini player': A,
    'Full player background': A,
    Backdrop: A,
    Shuffle: A,
    'Another one': A,
    Now: A,
    'Your picture': A,
    'Also on Home': A,
  },
};

/** Groups that go by their title, whole: their rows are things (a profile's sources, a crate's fit, this device's folders, the
 *  servers added), or a picker no row stands for. */
export const GROUPS: Partial<Record<SettingsSection, Readonly<Record<string, Tier>>>> = {
  taste: { Profile: A, 'Not for': A },
  device: { 'Local music': A, Servers: A },
  appearance: { 'Behind the flows': A },
};

/** A row's tier in a part, by the name it shows; null: no tier (a thing's row, or a part Basic does not touch). */
export function rowTier(part: SettingsSection, name: string): Tier | null {
  if (!TIERED.includes(part)) return null;
  if (ADVANCED_PARTS.includes(part)) return A;
  return ROWS[part]?.[name] ?? null;
}

/** A group's tier by its title; null: it shows as its rows do. */
export const groupTier = (part: SettingsSection, title: string | null | undefined): Tier | null => (title ? (GROUPS[part]?.[title] ?? null) : null);

/** Whether a part is listed in Basic: one Basic leaves alone, or one with a Basic row. */
export const listedInBasic = (part: SettingsSection) => !TIERED.includes(part) || !ADVANCED_PARTS.includes(part);

/** Whether Basic shows a row (and a Reset lists its item): a Basic one. A row with no tier is a thing's, and shows; an item of a
 *  Reset with none is no Basic row's, and is not listed. */
export const basicShows = (part: SettingsSection, name: string) => rowTier(part, name) !== A;
export const basicResets = (part: SettingsSection, name: string) => !TIERED.includes(part) || rowTier(part, name) === B;
