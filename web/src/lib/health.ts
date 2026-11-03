// The health report's findings on the page (decision 0063): where each opens, and the words for its domain. Read by the
// Health page (all of it) and Home's System status (the most critical and the most recent).
import type { HealthFinding } from './api';
import type { IconName } from './Icon.svelte';
import { setSwitch } from './listFilter.svelte';
import { hrefOf, isPage, pageHere, viewOn } from './addresses';

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

/** Health's three areas (the owner, 2026-11-01): what is yours (records, files, Incoming, playlists), what Cratarr
 *  talks to (servers, services, the network, sign-in, players, requests), and what it is doing (analysis; Changes to
 *  files beside it). Each domain is in one. */
export const AREAS = ['Library', 'Connections', 'Activity'] as const;
export type Area = (typeof AREAS)[number];
const AREA_OF: Record<string, Area> = {
  library: 'Library',
  incoming: 'Library',
  files: 'Library',
  playlists: 'Library',
  analysis: 'Activity',
};
export const areaOf = (domain: string): Area => AREA_OF[domain] ?? 'Connections';

/** Opens where a finding is dealt with (a switch turned on there first); none: nothing. */
export function openFinding(f: HealthFinding) {
  if (!f.open) return;
  if (f.open.page === 'library' && f.open.switch) setSwitch('library', f.open.switch, true);
  // A connection's own page in Settings › Connections.
  if (f.open.page === 'settings' && f.open.thing) return void (location.hash = viewOn('settings', `settings:connections/${f.open.thing}`));
  // The page the server names (lib/addresses); one the page does not have: nothing.
  if (isPage(f.open.page) && pageHere() !== f.open.page) location.hash = hrefOf(f.open.page);
}

/** Health with this finding open in its panel (Home's System status, the owner 2026-10-23); a files' finding is its kind's
 *  problem there (lib/healthReport's `files-<kind>`). */
export function openInHealth(f: HealthFinding) {
  openHealth(f.domain === 'files' ? `files-${f.key.split(':')[1]}` : f.key);
}
/** Health with one of its problems open (`library:incomplete`). */
export const openHealth = (key: string) => (location.hash = viewOn('health', `health:${key}`));

/** Its icon: its domain's. */
export const iconOf = (f: HealthFinding): IconName => DOMAINS[f.domain]?.icon ?? 'info';
