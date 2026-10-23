// The health report's findings on the page (decision 0063): where each opens, and the words for its domain. Read by the
// Health page (all of it) and Home's Needs you (the most critical and the most recent).
import type { HealthFinding } from './api';
import type { IconName } from './Icon.svelte';
import { setSwitch } from './listFilter.svelte';

/** Each domain's name and icon, in the Health page's order. */
export const DOMAINS: Record<string, { name: string; icon: IconName }> = {
  servers: { name: 'Servers', icon: 'wifi' },
  incoming: { name: 'Incoming', icon: 'arrowDown' },
  files: { name: 'Files', icon: 'music' },
  playlists: { name: 'Playlists', icon: 'playlists' },
  analysis: { name: 'Analysis', icon: 'gauge' },
  players: { name: 'Players', icon: 'speaker' },
  'sign-in': { name: 'Sign-in', icon: 'key' },
  server: { name: 'Cratarr', icon: 'health' },
};

const PAGES: Record<string, string> = { library: '#/library', playlists: '#/', settings: '#/settings', health: '#/health' };

/** Opens where a finding is dealt with (a switch turned on there first); none: nothing. */
export function openFinding(f: HealthFinding) {
  if (!f.open) return;
  if (f.open.page === 'library' && f.open.switch) setSwitch('library', f.open.switch, true);
  const to = PAGES[f.open.page];
  if (to && location.hash !== to) location.hash = to;
}

/** Its icon: its domain's. */
export const iconOf = (f: HealthFinding): IconName => DOMAINS[f.domain]?.icon ?? 'info';
