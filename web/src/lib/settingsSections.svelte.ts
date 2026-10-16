// The parts of the settings (decided 2026-10-16): the Settings page's cards, each opening its part in the panel
// (lib/SettingsPanel). `settingsSaved` counts the saves, so the cards read what they say again. `settingsRead` is the
// last answer the server gave: it checks every connection, which takes seconds, so a part opens on that at once.
import type { Settings } from './api';
import type { IconName } from './Icon.svelte';

export type SettingsSection = 'device' | 'connections' | 'libraries' | 'players' | 'subsonic' | 'display' | 'about';

export const SECTIONS: { id: SettingsSection; title: string; icon: IconName; app?: true }[] = [
  { id: 'device', title: 'This device', icon: 'speaker', app: true },
  { id: 'connections', title: 'Connections', icon: 'server' },
  { id: 'libraries', title: 'Libraries', icon: 'library' },
  { id: 'players', title: 'Players', icon: 'speakers' },
  { id: 'subsonic', title: 'Subsonic', icon: 'users' },
  { id: 'display', title: 'Display', icon: 'swatch' },
  { id: 'about', title: 'About', icon: 'info' },
];

export const settingsSaved = $state({ n: 0 });
export const settingsRead = $state({ s: null as Settings | null });
