// Display preferences: per browser, kept in localStorage (which may be unavailable; then defaults apply and nothing
// is remembered). Server-wide settings live on the server, see Settings.svelte.

import { inApp, toApp } from './app.svelte';
import { LIST_WIDTHS } from './pageView.svelte';
import type { Condition } from './api';
import { FACTS, type FactId } from './facts';

export type Theme = 'auto' | 'light' | 'dark';

export type Prefs = {
  theme: Theme;
  /** Index into the playlist chip widths (Playlists.svelte). */
  zoom: number;
  /** Index into a menu's playlists' widths (LIST_WIDTHS, Add to playlist): one column by default. */
  listZoom: number;
  /** A menu's playlists' order (its header, LIST_ORDERS), either way round. */
  listOrder: PlaylistOrder;
  listDown: boolean;
  /** Playlists the track is in come first. */
  showProgress: boolean;
  /** The full player's row under the title (the owner, 2026-10-19): shown at all, and the facts it shows beside its info
   *  button, in the user's order (lib/trackFacts; none at first). */
  playerRow: boolean;
  playerFacts: FactId[];
  /** A list's profile on top of it (the owner, 2026-10-23): open or folded, for every list at once; its rows switched
   *  on, in the user's order; and its fold line, how many of them stay when it is folded (0: none; Settings › Display ›
   *  AudioMuse). */
  profileOpen: boolean;
  profileRows: ProfileRow[];
  profileFoldAt: number;
  /** A playlist's and a crate's telling artists, a line under its genres (the owner, 2026-10-23: out of the profile,
   *  which keeps to what is heard and measured). */
  listArtists: boolean;
  /** Levelling (decision 0035): this browser's player plays every track about as loud as the next. */
  /** This browser's name as a Play On player, while a page plays here (lib/playOnTarget); '' until it has one. */
  browserName: string;
  levelling: boolean;
  /** View's Dates: an album's date added as how long ago ("3 days ago"), not the date. */
  relativeDates: boolean;
  /** Source this browser follows when several are active; '' = whichever started playing most recently. */
  source: string;
  /** Libraries switched off in the filter (`source/library`): remembered, as the server is (decided 2026-10-16). */
  hiddenLibraries: string[];
  /** Insights: the open tab, and which signals bring playlists together in Related. */
  /** Insights' tab (decision 0065). */
  insightsTab: 'collection' | 'listening' | 'map';
  /** Insights' tabs' own second rows (decision 0065): Collection's view, Listening's period. */
  insightsCollection: 'overview' | 'genres' | 'artists' | 'playlists';
  /** What Genres' and Artists' bars count: all the tracks, or those in a playlist (Insights' panel). */
  insightsCount: 'tracks' | 'listed';
  /** Insights' period, one for all of it (the owner, 2026-10-23): the past week, month or year, or all. Listening
   *  over it; Collection the albums that came in it; not the map. */
  insightsPeriod: 'week' | 'month' | 'year' | 'all';
  /** Listening's own sections (its second row): what was heard, or the taste it makes. */
  listeningView: 'history' | 'profile';
  signals: { tracks: boolean; genre: boolean; artists: boolean };
  /** The player's width when it shows beside the page (decision 0018). */
  playerWidth: number;
  /** Wide windows show the player beside the page (true) or the mini-player with the player over the page (false,
   *  after Maximise). */
  sidebar: boolean;
  /** The navigation folded to a strip of icons on a wide window (decision 0042). */
  navFolded: boolean;
  /** The part of the settings last open: Settings opens on it beside its list on a roomy window ('' = the first). */
  settingsPart: string;
  /** Each kind of list's columns in the owner's order (decision 0042, lib/columns), by id. */
  columns: Partial<Record<string, string[]>>;
  /** Where music plays (decisions 0019, 0022, 0031): on a server's player (`remote`), in this browser or app (`local`),
   *  or on one of Cratarr's own players by name (`player:Salon tablet`). */
  output: 'remote' | 'local' | `player:${string}`;
  /** Playlists pinned to the top of the Playlists page, in the order pinned (ids). */
  pinned: string[];
  /** The Playlists page's filter section (the filter and the order) is shown. */
  filterOpen: boolean;
  /** Insights → Related: its filter section (what brings playlists together) is shown. */
  relatedFilterOpen: boolean;
  /** The album lists (Incoming, Collection): rows or a grid of covers, and the size (an index into their widths). */
  albumLayout: AlbumLayout;
  albumZoom: number;
  /** The Library's order (decision 0034), and each page list's order turned round (descending), and Playlists' and
   *  Crates' own order. */
  libraryOrder: CollectionOrder;
  libraryDown: boolean;
  /** Downloads' order (the Library's orders), and turned round. */
  downloadsOrder: CollectionOrder;
  downloadsDown: boolean;
  playlistOrder: PlaylistOrder;
  playlistDown: boolean;
  /** The Playlists page as chips or as a table (decision 0042). */
  playlistLayout: 'chips' | 'table';
  crateOrder: CrateOrder;
  crateDown: boolean;
  /** The panel's track lists' order, per kind of list (decision 0060). */
  panelSorts: Partial<Record<PanelKind, ListSort>>;
  /** Every list's filter, per kind of list (a page's id, or a panel kind; decisions 0060 and 0061). */
  filters: Partial<Record<string, ListFilter>>;
};

export type AlbumLayout = 'rows' | 'grid' | 'table';
export type CollectionOrder = 'artist' | 'title' | 'year' | 'added' | 'plays' | 'played' | 'rating' | 'quality';
const COLLECTION_ORDERS: readonly CollectionOrder[] = ['artist', 'title', 'year', 'added', 'plays', 'played', 'rating', 'quality'];
export type PlaylistOrder = 'name' | 'tracks' | 'playing' | 'missing';
const PLAYLIST_ORDERS: readonly PlaylistOrder[] = ['name', 'tracks', 'playing', 'missing'];
export type CrateOrder = 'own' | 'name' | 'tracks' | 'length' | 'unplayed' | 'newWeek';
const CRATE_ORDERS: readonly CrateOrder[] = ['own', 'name', 'tracks', 'length', 'unplayed', 'newWeek'];
/** The panel's kinds of track list, each with its own remembered order and filter (decision 0060: per kind, not per
 *  list). */
export type PanelKind = 'playlist' | 'crate' | 'album' | 'artist';
/** A list's remembered order: a field of its kind (lib/pageView's tables), and turned round. */
export type ListSort = { key: string; down: boolean };
/** A list's remembered filter (decision 0061): its conditions (a crate's, unsaved; each can be muted) and its own
 *  switches that are on (lib/pageView's SWITCHES). */
export type ListFilter = { conditions: FilterCondition[]; on: string[] };
/** A filter's condition: a crate's, which can be muted (`enabled` false, kept but not applied), and over items that
 *  hold tracks tested on any or all of them (decision 0061; absent: on, any). */
export type FilterCondition = Condition & { enabled?: boolean; quantifier?: 'any' | 'all' };

/** A list profile's rows (lib/ListProfile), in its order: the server's number keys and the word rows. */
export type ProfileRow = 'style' | 'tempo' | 'energy' | 'danceability' | 'year' | 'mood';
export const PROFILE_ROWS: { id: ProfileRow; label: string }[] = [
  { id: 'style', label: 'Style' },
  { id: 'tempo', label: 'Tempo' },
  { id: 'energy', label: 'Energy' },
  { id: 'danceability', label: 'Dance' },
  { id: 'year', label: 'Era' },
  { id: 'mood', label: 'Mood' },
];

const KEY = 'cratarr-prefs';
export const ZOOM_STEPS = 6;
export const DEFAULTS: Prefs = {
  theme: 'auto',
  zoom: 2,
  listZoom: LIST_WIDTHS.length - 1,
  listOrder: 'playing',
  listDown: false,
  showProgress: true,
  playerRow: true,
  playerFacts: [],
  profileOpen: true,
  profileFoldAt: 0,
  listArtists: true,
  profileRows: PROFILE_ROWS.map((r) => r.id),
  levelling: true,
  browserName: '',
  relativeDates: false,
  source: '',
  hiddenLibraries: [],
  insightsTab: 'collection',
  insightsCollection: 'overview',
  insightsCount: 'tracks',
  insightsPeriod: 'all',
  listeningView: 'history',
  signals: { tracks: true, genre: true, artists: true },
  playerWidth: 400,
  sidebar: true,
  navFolded: false,
  settingsPart: '',
  columns: {},
  // This browser (it falls back to a player where a browser cannot play): Cratarr's own source has no server-side
  // player, which a page's first play went to, and failed (2026-10-16).
  output: 'local',
  pinned: [],
  filterOpen: true,
  relatedFilterOpen: true,
  albumLayout: 'rows',
  albumZoom: 2,
  // Newest first, as Incoming always showed, so a new arrival is on top.
  libraryOrder: 'added',
  libraryDown: true,
  downloadsOrder: 'artist',
  downloadsDown: false,
  playlistOrder: 'name',
  playlistDown: false,
  playlistLayout: 'chips',
  crateOrder: 'own',
  crateDown: false,
  panelSorts: {},
  filters: {},
};

function read(): Prefs {
  const p = { ...DEFAULTS, signals: { ...DEFAULTS.signals }, pinned: [] as string[], hiddenLibraries: [] as string[], panelSorts: {}, filters: {} } as Prefs;
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (saved && typeof saved === 'object') {
      if (['auto', 'light', 'dark'].includes(saved.theme)) p.theme = saved.theme;
      if (Number.isInteger(saved.zoom) && saved.zoom >= 0 && saved.zoom < ZOOM_STEPS) p.zoom = saved.zoom;
      if (Number.isInteger(saved.listZoom) && saved.listZoom >= 0 && saved.listZoom < LIST_WIDTHS.length) p.listZoom = saved.listZoom;
      if (typeof saved.filterOpen === 'boolean') p.filterOpen = saved.filterOpen;
      if (typeof saved.relatedFilterOpen === 'boolean') p.relatedFilterOpen = saved.relatedFilterOpen;
      if (saved.albumLayout === 'rows' || saved.albumLayout === 'grid' || saved.albumLayout === 'table') p.albumLayout = saved.albumLayout;
      if (Number.isInteger(saved.albumZoom) && saved.albumZoom >= 0 && saved.albumZoom < ZOOM_STEPS) p.albumZoom = saved.albumZoom;
      if (COLLECTION_ORDERS.includes(saved.libraryOrder)) p.libraryOrder = saved.libraryOrder;
      if (COLLECTION_ORDERS.includes(saved.downloadsOrder)) p.downloadsOrder = saved.downloadsOrder;
      if (PLAYLIST_ORDERS.includes(saved.playlistOrder)) p.playlistOrder = saved.playlistOrder;
      if (saved.playlistLayout === 'chips' || saved.playlistLayout === 'table') p.playlistLayout = saved.playlistLayout;
      if (PLAYLIST_ORDERS.includes(saved.listOrder) && saved.listOrder !== 'missing') p.listOrder = saved.listOrder;
      if (typeof saved.listDown === 'boolean') p.listDown = saved.listDown;
      // Before the list header: "✓ first" was the playlists' only other order.
      else if (saved.membersFirst === true) p.playlistOrder = 'playing';
      if (CRATE_ORDERS.includes(saved.crateOrder)) p.crateOrder = saved.crateOrder;
      for (const k of ['libraryDown', 'downloadsDown', 'playlistDown', 'crateDown'] as const) {
        if (typeof saved[k] === 'boolean') p[k] = saved[k];
      }
      if (typeof saved.showProgress === 'boolean') p.showProgress = saved.showProgress;
      if (typeof saved.playerRow === 'boolean') p.playerRow = saved.playerRow;
      if (Array.isArray(saved.playerFacts)) p.playerFacts = saved.playerFacts.filter((x: unknown): x is FactId => FACTS.some((f) => f.id === x));
      if (typeof saved.profileOpen === 'boolean') p.profileOpen = saved.profileOpen;
      if (Array.isArray(saved.profileRows)) p.profileRows = saved.profileRows.filter((x: unknown): x is ProfileRow => PROFILE_ROWS.some((r) => r.id === x));
      if (Number.isInteger(saved.profileFoldAt) && saved.profileFoldAt >= 0) {
        // Rows no longer in the profile (Artists, 2026-10-23) that were above the fold line no longer count there.
        const gone = Array.isArray(saved.profileRows) ? saved.profileRows.slice(0, saved.profileFoldAt).filter((x: unknown) => !PROFILE_ROWS.some((r) => r.id === x)).length : 0;
        p.profileFoldAt = Math.min(saved.profileFoldAt - gone, p.profileRows.length);
      }
      // Artists was a profile row: switched off there, the line stays off.
      if (typeof saved.listArtists === 'boolean') p.listArtists = saved.listArtists;
      else if (Array.isArray(saved.profileRows) && saved.profileRows.length && !saved.profileRows.includes('artists')) p.listArtists = false;
      if (typeof saved.levelling === 'boolean') p.levelling = saved.levelling;
      if (typeof saved.browserName === 'string') p.browserName = saved.browserName;
      if (typeof saved.relativeDates === 'boolean') p.relativeDates = saved.relativeDates;
      if (typeof saved.source === 'string') p.source = saved.source;
      if (Number.isFinite(saved.playerWidth) && saved.playerWidth >= 275) p.playerWidth = saved.playerWidth;
      if (typeof saved.sidebar === 'boolean') p.sidebar = saved.sidebar;
      if (typeof saved.navFolded === 'boolean') p.navFolded = saved.navFolded;
      if (typeof saved.settingsPart === 'string') p.settingsPart = saved.settingsPart;
      if (saved.columns && typeof saved.columns === 'object')
        for (const [k, v] of Object.entries(saved.columns))
          if (Array.isArray(v)) p.columns[k] = v.filter((x: unknown): x is string => typeof x === 'string');
      if (saved.output === 'remote' || saved.output === 'local' || (typeof saved.output === 'string' && saved.output.startsWith('player:')))
        p.output = saved.output;
      if (Array.isArray(saved.pinned)) p.pinned = saved.pinned.filter((x: unknown): x is string => typeof x === 'string');
      if (Array.isArray(saved.hiddenLibraries)) p.hiddenLibraries = saved.hiddenLibraries.filter((x: unknown): x is string => typeof x === 'string');
      if (saved.panelSorts && typeof saved.panelSorts === 'object')
        for (const [k, v] of Object.entries(saved.panelSorts) as [PanelKind, ListSort][])
          if (typeof v?.key === 'string' && typeof v.down === 'boolean') p.panelSorts[k] = { key: v.key, down: v.down };
      if (saved.filters && typeof saved.filters === 'object')
        for (const [k, v] of Object.entries(saved.filters) as [string, ListFilter][])
          if (Array.isArray(v?.conditions) && Array.isArray(v.on))
            p.filters[k] = { conditions: v.conditions.filter((c) => typeof c?.axis === 'string' && c.value), on: v.on.filter((x) => typeof x === 'string') };
      // The tabs' names before decision 0065: Statistics is Collection, Related is Map.
      const tab = saved.insightsTab === 'statistics' ? 'collection' : saved.insightsTab === 'related' ? 'map' : saved.insightsTab;
      if (tab === 'collection' || tab === 'listening' || tab === 'map') p.insightsTab = tab;
      if (['overview', 'genres', 'artists', 'playlists'].includes(saved.insightsCollection)) p.insightsCollection = saved.insightsCollection;
      if (saved.insightsCount === 'tracks' || saved.insightsCount === 'listed') p.insightsCount = saved.insightsCount;
      if (['week', 'month', 'year', 'all'].includes(saved.insightsPeriod)) p.insightsPeriod = saved.insightsPeriod;
      if (saved.listeningView === 'history' || saved.listeningView === 'profile') p.listeningView = saved.listeningView;
      if (saved.signals && typeof saved.signals === 'object') {
        for (const k of ['tracks', 'genre', 'artists'] as const) {
          if (typeof saved.signals[k] === 'boolean') p.signals[k] = saved.signals[k];
        }
      }
    } else {
      // Before there were preferences, only the zoom was remembered.
      const z = Number(localStorage.getItem('cratarr-zoom') ?? NaN);
      if (Number.isInteger(z) && z >= 0 && z < ZOOM_STEPS) p.zoom = z;
    }
  } catch {
    /* defaults */
  }
  return p;
}

export const prefs = $state<Prefs>(read());

/** Back to the default look; the followed source and the pins are not display matters and stay. */
export function resetPrefs() {
  Object.assign(prefs, { ...DEFAULTS, signals: { ...DEFAULTS.signals }, source: prefs.source, hiddenLibraries: prefs.hiddenLibraries, pinned: prefs.pinned, panelSorts: {}, filters: {} });
}

/** Pin a playlist to the top of the Playlists page (after those pinned before it), or unpin it. */
export function togglePin(id: string) {
  prefs.pinned = prefs.pinned.includes(id) ? prefs.pinned.filter((p) => p !== id) : [...prefs.pinned, id];
}

// Remember every change and apply the theme (app.css reads data-theme; no attribute = follow the system).
$effect.root(() => {
  $effect(() => {
    const snapshot = JSON.stringify(prefs);
    try {
      localStorage.setItem(KEY, snapshot);
    } catch {
      /* not remembered */
    }
    // In the app the same settings go to it too: it hands them to its pages on either of its two addresses (Network).
    if (inApp) toApp({ type: 'page-prefs', prefs: snapshot });
    if (prefs.theme === 'auto') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.dataset.theme = prefs.theme;
  });
});
