// What Insights' panel holds (the owner, 2026-10-23: "a standardized way of implementing these controls in the insights
// panel"): each tab says what it offers, as groups of controls of a few kinds, and the panel (lib/InsightsPanel) draws
// every kind one way, as the settings' rows (lib/settings/Group, Item). A tab adds a control here, never markup there.
// Collection: its libraries, one switch each (the page's library switches, as on Library and Crates; the last one on
// stays on); on Genres and Artists, what the bars count. Listening: the period. Map: what brings playlists together.
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

function collection(): ControlGroup[] {
  askLibraries();
  // The server the figures come from: the one followed, else the first (as the server picks it).
  const sid = prefs.source || known.list[0]?.id;
  const view = libraries.view?.sources.find((s) => s.id === sid);
  if (!sid || !view) return [];
  const used = view.server_libraries.filter((l) => view.libraries.music.includes(l.id) || view.libraries.incoming.includes(l.id));
  const on = used.filter((l) => libraryShown(sid, l.id)).length;
  const counted: ControlGroup[] =
    prefs.insightsCollection === 'genres' || prefs.insightsCollection === 'artists'
      ? [{ title: null, controls: [{ kind: 'choice', name: 'Count', options: COUNTS, get: () => prefs.insightsCount, set: (v) => (prefs.insightsCount = v as typeof prefs.insightsCount) }] }]
      : [];
  return [
    ...counted,
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

const PERIODS = [
  { value: 'week', label: 'Week' },
  { value: 'month', label: 'Month' },
  { value: 'year', label: 'Year' },
];
// Profile has nothing to choose yet: no panel button there.
const listening = (): ControlGroup[] =>
  prefs.listeningView === 'profile'
    ? []
    : [
  {
    title: null,
    controls: [{ kind: 'choice', name: 'Period', options: PERIODS, get: () => prefs.listeningPeriod, set: (v) => (prefs.listeningPeriod = v as typeof prefs.listeningPeriod) }],
  },
];

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

/** The open tab's controls. */
export const insightsControls = (): ControlGroup[] =>
  prefs.insightsTab === 'map' ? map() : prefs.insightsTab === 'listening' ? listening() : collection();
