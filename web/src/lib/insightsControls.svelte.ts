// What Insights' panel holds (the owner, 2026-10-23: "a standardized way of implementing these controls in the insights
// panel"): each tab says what it offers, as groups of controls of a few kinds, and the panel (lib/InsightsPanel) draws
// every kind one way, as the settings' rows (lib/settings/Group, Item). A tab adds a control here, never markup there.
// Every tab: the period (not the map) and the libraries, one switch each (Insights' own, the same everywhere; the last
// one on stays on). Collection's Genres, Artists and Tracks: what they count (each its own ranking of the whole set, the server's for Tracks'
// top lists); Genres and Artists a sort. Profile: what it is built from. Map: what
// brings playlists together.
import { getLibraries, type LibrariesView } from './api';
import { prefs } from './prefs.svelte';
import { known, libraryShown, showLibrary } from './sources.svelte';

/** One of a few (a segmented switch), or on and off (a switch). */
export type Control =
  | { kind: 'choice'; name: string; options: { value: string; label: string }[]; get: () => string; set: (v: string) => void }
  | { kind: 'toggle'; name: string; line?: string; get: () => boolean; set: (on: boolean) => void; disabled?: boolean };
export type ControlGroup = { title: string | null; controls: Control[] };

/** The servers' libraries and their uses, asked for once, when Collection's controls are first shown. */
const libraries = $state<{ view: LibrariesView | null }>({ view: null });
let asked = false;
function askLibraries() {
  if (asked) return;
  asked = true;
  getLibraries().then(
    (v) => (libraries.view = v),
    () => (asked = false),
  );
}

const COUNTS = [
  { value: 'tracks', label: 'Tracks' },
  { value: 'listed', label: 'In a playlist' },
];

/** The libraries, one switch each: Insights' own, the same in every tab's panel (the owner, 2026-10-23: "the library
 *  switches are like global for the insights"), and the page's library switches as on Library and Crates. */
function libraryGroup(): ControlGroup[] {
  askLibraries();
  // The server the figures come from: the one followed, else the first (as the server picks it).
  const sid = prefs.source || known.list[0]?.id;
  const view = libraries.view?.sources.find((s) => s.id === sid);
  if (!sid || !view) return [];
  const used = view.server_libraries.filter((l) => view.libraries.music.includes(l.id) || view.libraries.incoming.includes(l.id));
  const on = used.filter((l) => libraryShown(sid, l.id)).length;
  return [
    {
      title: 'Libraries',
      controls: used.map((l) => ({
        kind: 'toggle',
        name: l.name,
        line: view.libraries.incoming.includes(l.id) ? 'Incoming' : 'Music',
        get: () => libraryShown(sid, l.id),
        set: (v: boolean) => showLibrary(sid, l.id, v),
        disabled: on === 1 && libraryShown(sid, l.id),
      })),
    },
  ];
}

const TRACK_COUNTS = [
  { value: 'listed', label: 'In a playlist' },
  { value: 'plays', label: 'Plays' },
];
const SORTS = [
  { value: 'most', label: 'Most first' },
  { value: 'az', label: 'A–Z' },
];
const sort: Control = { kind: 'choice', name: 'Sort', options: SORTS, get: () => prefs.insightsSort, set: (v) => (prefs.insightsSort = v as typeof prefs.insightsSort) };
// The long lists get a sort (the owner, 2026-10-23); Tracks', History's and Profile's are top lists, where it would mean
// nothing.
const collection = (): ControlGroup[] =>
  prefs.insightsCollection === 'genres' || prefs.insightsCollection === 'artists'
    ? [{ title: null, controls: [{ kind: 'choice', name: 'Count', options: COUNTS, get: () => prefs.insightsCount, set: (v) => (prefs.insightsCount = v as typeof prefs.insightsCount) }, sort] }]
    : prefs.insightsCollection === 'tracks'
      ? [{ title: null, controls: [{ kind: 'choice', name: 'Count', options: TRACK_COUNTS, get: () => prefs.tracksCount, set: (v) => (prefs.tracksCount = v as typeof prefs.tracksCount) }] }]
      : [];

const PERIODS = [
  { value: 'week', label: 'Week' },
  { value: 'month', label: 'Month' },
  { value: 'year', label: 'Year' },
  { value: 'all', label: 'All' },
];
/** The period: Insights' own, the same everywhere but the map (which is how the playlists relate now). */
const periodGroup = (): ControlGroup[] =>
  prefs.insightsTab === 'map'
    ? []
    : [{ title: null, controls: [{ kind: 'choice', name: 'Period', options: PERIODS, get: () => prefs.insightsPeriod, set: (v) => (prefs.insightsPeriod = v as typeof prefs.insightsPeriod) }] }];

// Profile's sources (the owner, 2026-10-23: the one tab with an algorithm worth choosing the inputs of, beside the map);
// Settings' Taste switches the same ones (lib/settings/TastePart).
export const PROFILE_SOURCES = [
  { id: 'favourites', label: 'Favourites', line: 'More of' },
  { id: 'playlists', label: 'Playlists', line: 'More of' },
  { id: 'plays', label: 'Plays', line: 'More of' },
  { id: 'skipped', label: 'Skips', line: 'Less of' },
  { id: 'excluded', label: 'Exclusions', line: 'Less of' },
] as const;
const profile = (): ControlGroup[] =>
  prefs.insightsTab === 'listening' && prefs.listeningView === 'profile'
    ? [
        {
          title: 'Built from',
          controls: PROFILE_SOURCES.map((s) => ({ kind: 'toggle', name: s.label, line: s.line, get: () => prefs.profileSources[s.id], set: (on: boolean) => (prefs.profileSources[s.id] = on) })),
        },
      ]
    : [];

const SIGNALS = [
  { id: 'tracks', label: 'Shared tracks' },
  { id: 'genre', label: 'Genre' },
  { id: 'artists', label: 'Artists' },
] as const;
const map = (): ControlGroup[] => [
  {
    title: 'Connected by',
    controls: SIGNALS.map((s) => ({ kind: 'toggle', name: s.label, get: () => prefs.signals[s.id], set: (on: boolean) => (prefs.signals[s.id] = on) })),
  },
];

/** The period, the open tab's own controls, then the libraries. */
export const insightsControls = (): ControlGroup[] => [...insightsFilter(), ...insightsView()];

/** The pill's two (the owner, 2026-10-25, as the pages' pill): the filter, what is counted (the period, the profile's
 *  sources, the libraries); and View, how it is shown (the sort and the count, what connects the map). */
// Discovery has neither: Discover's list is as its last look found it, beyond the libraries.
const discovery = () => prefs.insightsTab === 'collection' && prefs.insightsCollection === 'discover';
export const insightsFilter = (): ControlGroup[] =>
  discovery() ? [] : [...periodGroup(), ...(prefs.insightsTab === 'listening' ? profile() : []), ...libraryGroup()];
export const insightsView = (): ControlGroup[] => (prefs.insightsTab === 'map' ? map() : prefs.insightsTab === 'collection' ? collection() : []);
