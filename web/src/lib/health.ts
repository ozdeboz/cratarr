// The health report's findings on the page (decision 0063): where each opens, and the words for its domain. Read by the
// Health page (all of it) and Home's System status (the most critical and the most recent).
import type { HealthFinding } from './api';
import type { IconName } from './Icon.svelte';
import { setSwitch } from './listFilter.svelte';

/** Each domain's name and icon, in the Health page's order. */
export const DOMAINS: Record<string, { name: string; icon: IconName }> = {
  servers: { name: 'Servers', icon: 'wifi' },
  // The services Cratarr asks (0.9.6): each one failing (the owner, 2026-10-26: every connection's problems).
  connections: { name: 'Connections', icon: 'server' },
  library: { name: 'Library', icon: 'library' },
  incoming: { name: 'Incoming', icon: 'arrowDown' },
  files: { name: 'Files', icon: 'music' },
  playlists: { name: 'Playlists', icon: 'playlists' },
  analysis: { name: 'Analysis', icon: 'gauge' },
  players: { name: 'Players', icon: 'speaker' },
  'sign-in': { name: 'Sign-in', icon: 'key' },
  // Its answers and the clients asking it (the owner, 2026-10-25: "That group should be Network").
  server: { name: 'Network', icon: 'health' },
  requests: { name: 'Requests', icon: 'inbox' },
};

const PAGES: Record<string, string> = { library: '#/library', playlists: '#/', settings: '#/settings', health: '#/health', requests: '#/requests' };

/** Opens where a finding is dealt with (a switch turned on there first); none: nothing. */
export function openFinding(f: HealthFinding) {
  if (!f.open) return;
  if (f.open.page === 'library' && f.open.switch) setSwitch('library', f.open.switch, true);
  // A connection's own page in Settings › Connections.
  if (f.open.page === 'settings' && f.open.thing) return void (location.hash = `#/settings?view=${encodeURIComponent(`settings:connections/${f.open.thing}`)}`);
  const to = PAGES[f.open.page];
  if (to && location.hash !== to) location.hash = to;
}

/** Health with this finding open in its panel (Home's System status, the owner 2026-10-23); a files' finding is its kind's
 *  problem there (lib/healthReport's `files-<kind>`). */
export function openInHealth(f: HealthFinding) {
  const key = f.domain === 'files' ? `files-${f.key.split(':')[1]}` : f.key;
  location.hash = `#/health?view=${encodeURIComponent(`health:${key}`)}`;
}

/** Its icon: its domain's. */
export const iconOf = (f: HealthFinding): IconName => DOMAINS[f.domain]?.icon ?? 'info';
