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
  | 'display'
  | 'taste'
  | 'home'
  | 'connections'
  | 'libraries'
  | 'players'
  | 'users'
  | 'auth'
  | 'diagnostics';

/** `admin`: the server's, its administrators' only; `everyone` as well: a part anyone opens, showing them only their own
 *  (Connections' listening services, decision 0068). */
export const SECTIONS: { id: SettingsSection; title: string; icon: IconName; app?: true; admin?: true; everyone?: true }[] = [
  // Personal, in the owner's order (2026-10-25): This device, Account, Network, Sync, Display, Home, Taste.
  // In a browser too: its name as a Play On player.
  { id: 'device', title: 'This device', icon: 'speaker' },
  // Who is signed in: their role, their password for OpenSubsonic apps, their devices and sign-ins.
  { id: 'account', title: 'Account', icon: 'user' },
  { id: 'network', title: 'Network', icon: 'wifi', app: true },
  { id: 'sync', title: 'Sync', icon: 'arrowDownToLine', app: true },
  { id: 'display', title: 'Display', icon: 'swatch' },
  // Home's sections, their order, and its header (the owner, 2026-10-24).
  { id: 'home', title: 'Home', icon: 'home' },
  // What one has said one does not want (the owner, 2026-10-23): every exclusion, managed in one place.
  { id: 'taste', title: 'Taste', icon: 'music' },
  { id: 'connections', title: 'Connections', icon: 'server', admin: true, everyone: true },
  { id: 'libraries', title: 'Libraries', icon: 'library', admin: true },
  { id: 'players', title: 'Players', icon: 'speakers', admin: true },
  { id: 'users', title: 'Users', icon: 'users', admin: true },
  { id: 'auth', title: 'Authentication', icon: 'key', admin: true },
  // Another server's plays and favourites into an account: its administrators' (listening is private).
  { id: 'import', title: 'Import', icon: 'intoBox', admin: true },
  // What happened, as Cratarr keeps it (decision 0045).
  { id: 'diagnostics', title: 'Diagnostics', icon: 'list', admin: true },
];

export const settingsSaved = $state({ n: 0 });
/** Diagnostics' More detail, on or off as last read or set (null: not read yet): its row says it beside the part. */
export const logDetail = $state({ on: null as boolean | null });
/** The name of the thing whose own page a part shows (lib/SettingsPanel's title there). */
export const thingTitle = $state({ text: null as string | null });
/** A part names the thing its page shows, as long as it shows it (called while the part is made). */
export function nameThing(name: () => string | null) {
  $effect(() => {
    thingTitle.text = name();
  });
}
export const settingsRead = $state({ s: null as Settings | null });
