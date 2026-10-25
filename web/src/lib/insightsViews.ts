// Insights' views (the owner, 2026-10-24: "the Insights page should really follow the Health and Settings idea"): a
// list on its page, each view opening in the panel (`insights:<view>`, lib/InsightsPanel), its controls in a sheet
// (lib/InsightsSheet). Which view shows is kept in the prefs the views already read (the tab and its section), so the
// one last open is opened again.
import type { IconName } from './Icon.svelte';
import { prefs } from './prefs.svelte';

export type InsightsView = 'overview' | 'genres' | 'artists' | 'tracks' | 'discover' | 'discover-missing' | 'discover-excluded' | 'history' | 'profile' | 'map';
export type InsightsGroup = 'Collection' | 'Listening' | 'Playlists';

/** `under`: a view one level deeper, opened from that view (its breadcrumb's level above), not in the page's list. */
export const INSIGHTS_VIEWS: { id: InsightsView; group: InsightsGroup; label: string; icon: IconName; under?: InsightsView }[] = [
  { id: 'overview', group: 'Collection', label: 'Overview', icon: 'layoutGrid' },
  { id: 'genres', group: 'Collection', label: 'Genres', icon: 'swatch' },
  { id: 'artists', group: 'Collection', label: 'Artists', icon: 'mic' },
  { id: 'tracks', group: 'Collection', label: 'Tracks', icon: 'music' },
  // What Discover finds beyond the collection (the owner, 2026-10-25: "a discovery panel under collection insights").
  { id: 'discover', group: 'Collection', label: 'Discovery', icon: 'compass' },
  // Its two lists (the owner, 2026-10-25: tiles leading one level deeper).
  { id: 'discover-missing', group: 'Collection', label: 'Missing albums', icon: 'compass', under: 'discover' },
  { id: 'discover-excluded', group: 'Collection', label: 'Discovery exclusions', icon: 'compass', under: 'discover' },
  { id: 'history', group: 'Listening', label: 'History', icon: 'insights' },
  { id: 'profile', group: 'Listening', label: 'Profile', icon: 'user' },
  { id: 'map', group: 'Playlists', label: 'Map', icon: 'share' },
];
export const INSIGHTS_GROUPS: InsightsGroup[] = ['Collection', 'Listening', 'Playlists'];

/** Shows `view`: the tab and its section the views and their controls read. */
export function showView(view: InsightsView) {
  // A view under another shows as that one does (its controls: none for Discovery's).
  const parent = INSIGHTS_VIEWS.find((v) => v.id === view)?.under;
  if (parent) view = parent;
  if (view === 'map') prefs.insightsTab = 'map';
  else if (view === 'history' || view === 'profile') {
    prefs.insightsTab = 'listening';
    prefs.listeningView = view;
  } else {
    prefs.insightsTab = 'collection';
    prefs.insightsCollection = view as typeof prefs.insightsCollection;
  }
}

/** The view last shown. */
export const viewNow = (): InsightsView =>
  prefs.insightsTab === 'map' ? 'map' : prefs.insightsTab === 'listening' ? prefs.listeningView : prefs.insightsCollection;
