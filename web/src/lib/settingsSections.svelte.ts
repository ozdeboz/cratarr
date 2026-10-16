// The parts of the settings (decided 2026-10-16): the Settings page's cards, each opening its part in the panel
// (lib/SettingsPanel). `settingsSaved` counts the saves, so the cards read what they say again. `settingsRead` is the
// last answer the server gave: it checks every connection, which takes seconds, so a part opens on that at once.
import type { Settings } from './api';
import type { IconName } from './Icon.svelte';

export type SettingsSection = 'device' | 'network' | 'sync' | 'connections' | 'libraries' | 'players' | 'auth' | 'import' | 'display';

export const SECTIONS: { id: SettingsSection; title: string; icon: IconName; app?: true }[] = [
  // In a browser too: its name as a Play On player.
  { id: 'device', title: 'This device', icon: 'speaker' },
  { id: 'network', title: 'Network', icon: 'wifi', app: true },
  { id: 'sync', title: 'Sync', icon: 'arrowDownToLine', app: true },
  { id: 'connections', title: 'Connections', icon: 'server' },
  { id: 'libraries', title: 'Libraries', icon: 'library' },
  { id: 'players', title: 'Players', icon: 'speakers' },
  { id: 'auth', title: 'Authentication', icon: 'user' },
  { id: 'import', title: 'Import', icon: 'intoBox' },
  { id: 'display', title: 'Display', icon: 'swatch' },
];

export const settingsSaved = $state({ n: 0 });
export const settingsRead = $state({ s: null as Settings | null });
