// Insights' views (the owner, 2026-10-24: "the Insights page should really follow the Health and Settings idea"): a
// list on its page, each view opening in the panel (`insights:<view>`, lib/InsightsPanel), its controls in a sheet
// (lib/InsightsSheet). Which view shows is kept in the prefs the views already read (the tab and its section), so the
// one last open is opened again.
import type { IconName } from './Icon.svelte';
import { prefs } from './prefs.svelte';

export type InsightsView = 'overview' | 'genres' | 'artists' | 'tracks' | 'discover' | 'history' | 'profile' | 'map';
export type InsightsGroup = 'Collection' | 'Listening' | 'Playlists';

export const INSIGHTS_VIEWS: { id: InsightsView; group: InsightsGroup; label: string; icon: IconName }[] = [
  { id: 'overview', group: 'Collection', label: 'Overview', icon: 'layoutGrid' },
  { id: 'genres', group: 'Collection', label: 'Genres', icon: 'swatch' },
  { id: 'artists', group: 'Collection', label: 'Artists', icon: 'mic' },
  { id: 'tracks', group: 'Collection', label: 'Tracks', icon: 'music' },
  // What Discover finds beyond the collection (the owner, 2026-10-25: "a discovery panel under collection insights").
  { id: 'discover', group: 'Collection', label: 'Discovery', icon: 'compass' },
  { id: 'history', group: 'Listening', label: 'History', icon: 'insights' },
  { id: 'profile', group: 'Listening', label: 'Profile', icon: 'user' },
  { id: 'map', group: 'Playlists', label: 'Map', icon: 'share' },
];
export const INSIGHTS_GROUPS: InsightsGroup[] = ['Collection', 'Listening', 'Playlists'];

/** Shows `view`: the tab and its section the views and their controls read. */
export function showView(view: InsightsView) {
  if (view === 'map') prefs.insightsTab = 'map';
  else if (view === 'history' || view === 'profile') {
    prefs.insightsTab = 'listening';
    prefs.listeningView = view;
  } else {
    prefs.insightsTab = 'collection';
    prefs.insightsCollection = view;
  }
}

/** The view last shown. */
export const viewNow = (): InsightsView =>
  prefs.insightsTab === 'map' ? 'map' : prefs.insightsTab === 'listening' ? prefs.listeningView : prefs.insightsCollection;
