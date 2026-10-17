// Display preferences: per browser, kept in localStorage (which may be unavailable; then defaults apply and nothing
// is remembered). Server-wide settings live on the server, see Settings.svelte.

import { inApp, toApp } from './app.svelte';
import { LIST_WIDTHS } from './pageView.svelte';

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
  insightsTab: 'statistics' | 'related';
  signals: { tracks: boolean; genre: boolean; artists: boolean };
  /** The player's width when it shows beside the page (decision 0018). */
  playerWidth: number;
  /** Wide windows show the player beside the page (true) or the mini-player with the player over the page (false,
   *  after Maximise). */
  sidebar: boolean;
  /** The navigation folded to a strip of icons on a wide window (decision 0042). */
  navFolded: boolean;
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
};

export type AlbumLayout = 'rows' | 'grid' | 'table';
export type CollectionOrder = 'artist' | 'title' | 'year' | 'added' | 'plays' | 'played' | 'rating';
const COLLECTION_ORDERS: readonly CollectionOrder[] = ['artist', 'title', 'year', 'added', 'plays', 'played', 'rating'];
export type PlaylistOrder = 'name' | 'tracks' | 'playing';
export type CrateOrder = 'own' | 'name';

const KEY = 'cratarr-prefs';
export const ZOOM_STEPS = 6;
export const DEFAULTS: Prefs = {
  theme: 'auto',
  zoom: 2,
  listZoom: LIST_WIDTHS.length - 1,
  listOrder: 'playing',
  listDown: false,
  showProgress: true,
  levelling: true,
  browserName: '',
  relativeDates: false,
  source: '',
  hiddenLibraries: [],
  insightsTab: 'statistics',
  signals: { tracks: true, genre: true, artists: true },
  playerWidth: 400,
  sidebar: true,
  navFolded: false,
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
};

function read(): Prefs {
  const p = { ...DEFAULTS, signals: { ...DEFAULTS.signals }, pinned: [] as string[], hiddenLibraries: [] as string[] };
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
      if (['name', 'tracks', 'playing'].includes(saved.playlistOrder)) p.playlistOrder = saved.playlistOrder;
      if (saved.playlistLayout === 'chips' || saved.playlistLayout === 'table') p.playlistLayout = saved.playlistLayout;
      if (['name', 'tracks', 'playing'].includes(saved.listOrder)) p.listOrder = saved.listOrder;
      if (typeof saved.listDown === 'boolean') p.listDown = saved.listDown;
      // Before the list header: "✓ first" was the playlists' only other order.
      else if (saved.membersFirst === true) p.playlistOrder = 'playing';
      if (['own', 'name'].includes(saved.crateOrder)) p.crateOrder = saved.crateOrder;
      for (const k of ['libraryDown', 'downloadsDown', 'playlistDown', 'crateDown'] as const) {
        if (typeof saved[k] === 'boolean') p[k] = saved[k];
      }
      if (typeof saved.showProgress === 'boolean') p.showProgress = saved.showProgress;
      if (typeof saved.levelling === 'boolean') p.levelling = saved.levelling;
      if (typeof saved.browserName === 'string') p.browserName = saved.browserName;
      if (typeof saved.relativeDates === 'boolean') p.relativeDates = saved.relativeDates;
      if (typeof saved.source === 'string') p.source = saved.source;
      if (Number.isFinite(saved.playerWidth) && saved.playerWidth >= 275) p.playerWidth = saved.playerWidth;
      if (typeof saved.sidebar === 'boolean') p.sidebar = saved.sidebar;
      if (typeof saved.navFolded === 'boolean') p.navFolded = saved.navFolded;
      if (saved.columns && typeof saved.columns === 'object')
        for (const [k, v] of Object.entries(saved.columns))
          if (Array.isArray(v)) p.columns[k] = v.filter((x: unknown): x is string => typeof x === 'string');
      if (saved.output === 'remote' || saved.output === 'local' || (typeof saved.output === 'string' && saved.output.startsWith('player:')))
        p.output = saved.output;
      if (Array.isArray(saved.pinned)) p.pinned = saved.pinned.filter((x: unknown): x is string => typeof x === 'string');
      if (Array.isArray(saved.hiddenLibraries)) p.hiddenLibraries = saved.hiddenLibraries.filter((x: unknown): x is string => typeof x === 'string');
      if (saved.insightsTab === 'statistics' || saved.insightsTab === 'related') p.insightsTab = saved.insightsTab;
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
  Object.assign(prefs, { ...DEFAULTS, signals: { ...DEFAULTS.signals }, source: prefs.source, hiddenLibraries: prefs.hiddenLibraries, pinned: prefs.pinned });
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
