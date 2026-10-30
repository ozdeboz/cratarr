// The parts of the settings (decided 2026-10-16): the Settings page's cards, each opening its part in the panel
// (lib/SettingsPanel). `settingsSaved` counts the saves, so the cards read what they say again. `settingsRead` is the
// last answer the server gave: it checks every connection, which takes seconds, so a part opens on that at once.
// Since several users (decision 0051) the parts are each user's own first, then the server's, which only its
// administrators see (`admin`).
import type { Settings } from './api';
import type { IconName } from './Icon.svelte';

export type SettingsSection =
  | 'device'
  | 'network'
  | 'sync'
  | 'account'
  | 'import'
  | 'serverimport'
  | 'appearance'
  | 'layout'
  | 'player'
  | 'taste'
  | 'radio'
  | 'tracking'
  | 'content'
  | 'connections'
  | 'libraries'
  | 'players'
  | 'users'
  | 'auth'
  | 'diagnostics';

/** Where a part is kept, its group on the Settings page (the owner, 2026-10-25): the account's own (Personal), this
 *  device's (Device: kept here, every device its own), the server's (Server). `admin`: the server's, its administrators'
 *  only; `everyone` as well: a part anyone opens, showing them only their own (Connections' listening services,
 *  decision 0068), Personal for someone not an administrator. */
export type SettingsKept = 'personal' | 'device' | 'server';
export const SECTIONS: { id: SettingsSection; title: string; icon: IconName; kept: SettingsKept; app?: true; admin?: true; everyone?: true }[] = [
  // Personal: who is signed in (their role, their password for OpenSubsonic apps, their devices and sign-ins), and what
  // they have said they do not want (the owner, 2026-10-23: every exclusion, in one place).
  { id: 'account', title: 'Account', icon: 'user', kept: 'personal' },
  { id: 'taste', title: 'Taste', icon: 'music', kept: 'personal' },
  // What Radio and Autoplay play and draw from, and whether plays are kept at all, with one's listening services (the
  // owner, 2026-10-25).
  { id: 'radio', title: 'Radio and Autoplay', icon: 'radio', kept: 'personal' },
  { id: 'tracking', title: 'Tracking', icon: 'resume', kept: 'personal' },
  // One's own listening from elsewhere (ListenBrainz, Last.fm, files) into one's own history, beside what is kept of it
  // (the owner, 2026-10-26: importing is a personal concern, not the server's).
  { id: 'import', title: 'Import', icon: 'intoBox', kept: 'personal' },
  // Device, in the owner's order: the device itself (named as Play On knows it; in a browser too), then how it reaches
  // Cratarr, what it keeps, and how it looks and lays out.
  { id: 'device', title: 'This device', icon: 'speaker', kept: 'device' },
  { id: 'network', title: 'Network', icon: 'wifi', kept: 'device', app: true },
  { id: 'sync', title: 'Sync', icon: 'arrowDownToLine', kept: 'device', app: true },
  { id: 'appearance', title: 'Appearance', icon: 'swatch', kept: 'device' },
  { id: 'layout', title: 'Layout', icon: 'layoutGrid', kept: 'device' },
  { id: 'player', title: 'Player', icon: 'play', kept: 'device' },
  // Home's sections and the music profiles (the owner, 2026-10-25).
  { id: 'content', title: 'Content', icon: 'home', kept: 'device' },
  { id: 'connections', title: 'Connections', icon: 'server', kept: 'server', admin: true },
  { id: 'libraries', title: 'Libraries', icon: 'library', kept: 'server', admin: true },
  // The server's devices (the owner, 2026-10-25: "players should be called devices"), its line still how many players.
  { id: 'players', title: 'Devices', icon: 'speakers', kept: 'server', admin: true },
  { id: 'users', title: 'Users', icon: 'users', kept: 'server', admin: true },
  { id: 'auth', title: 'Authentication', icon: 'key', kept: 'server', admin: true },
  // Another server's plays and favourites into an account: its administrators' (listening is private).
  { id: 'serverimport', title: 'Import from a server', icon: 'intoBox', kept: 'server', admin: true },
  // What happened, as Cratarr keeps it (decision 0045).
  { id: 'diagnostics', title: 'Diagnostics', icon: 'list', kept: 'server', admin: true },
];

export const settingsSaved = $state({ n: 0 });
/** The name of the thing whose own page a part shows (lib/SettingsPanel's title there). */
export const thingTitle = $state({ text: null as string | null });
/** A part names the thing its page shows, as long as it shows it (called while the part is made). */
export function nameThing(name: () => string | null) {
  $effect(() => {
    thingTitle.text = name();
  });
}
export const settingsRead = $state({ s: null as Settings | null });
