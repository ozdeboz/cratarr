// Display preferences: per browser, kept in localStorage (which may be unavailable; then defaults apply and nothing
// is remembered). Server-wide settings live on the server, see Settings.svelte.

import { inApp, toApp } from './app.svelte';
import { RECORD_ORDERS, type CrateView, type RecordOrder } from './crateRecords';
import { LIST_WIDTHS } from './pageView.svelte';
import { isDiscoverMode, isSpotifyView, type DiscoverMode } from './discoverModes';
import type { Condition } from './api';
import { FACTS, type FactId } from './facts';

/** The bands' Speed (lib/BandSlider): Ultra slow to Radical, times each look's own pace (Radical: four times the first
 *  Lively, Ultra slow a seventh of the first Calm; the owner, 2026-10-24). */
export const BAND_PACE = { min: 0.1, max: 10.4 };
/** The bands' Window (lib/BandSlider): the seconds of music around now their levels are of (the owner, 2026-10-24: a
 *  wider range than the first 1 to 8). */
export const BAND_WINDOW = { min: 0.25, max: 30 };
/** The bands' Cycles (times each band's cycles a second) and Height (times how tall loudness draws), lib/BandSlider. */
export const BAND_CYCLES = { min: 0.25, max: 4 };
export const BAND_HEIGHT = { min: 0.4, max: 2.5 };
/** How many bands the bands are drawn in: 3 (low, mid, high) to the 32 measured (the owner, 2026-10-24). */
export const BAND_COUNT = { min: 3, max: 32 };
/** How far apart the bands are drawn: 0 all on one line; 1 spread over the height (the meter: its bars apart). */
export const BAND_GAP = { min: 0, max: 1 };
/** Taper and Fade: none to full. */
export const BAND_ENDS_RANGE = { min: 0, max: 1 };
/** The looks the bands are drawn in (lib/BandGlass). */
export type BandLookName = 'aurora' | 'sirius' | 'hairlines' | 'meter';
export const BAND_LOOK_NAMES: BandLookName[] = ['aurora', 'sirius', 'hairlines', 'meter'];
/** A look's settings (each look its own: lib/bandTune). */
export type BandTune = {
  /** How fast it moves, times each look's own pace. */
  speed: number;
  /** Seconds of music: the waves' width, now in the middle; what the meter takes in up to now. */
  window: number;
  /** Times each band's cycles a second. */
  cycles: number;
  /** Times how tall loudness draws. */
  height: number;
  /** How many bands it is drawn in. */
  count: number;
  /** How far apart the bands are drawn (the meter: its bars). */
  gap: number;
  /** Which way the music flows (the meter: which side the bass is on). */
  direction: 'left' | 'right';
  /** How much the ends taper (the meter: its bars towards their tops), and fade, eased. */
  taper: number;
  fade: number;
};
/** Each number setting's range, for the sliders and for reading saved ones. */
export const TUNE_RANGES: Record<Exclude<keyof BandTune, 'direction'>, { min: number; max: number }> = {
  speed: BAND_PACE,
  window: BAND_WINDOW,
  cycles: BAND_CYCLES,
  height: BAND_HEIGHT,
  count: BAND_COUNT,
  gap: BAND_GAP,
  taper: BAND_ENDS_RANGE,
  fade: BAND_ENDS_RANGE,
};

/** The saved overrides, each checked; the settings from before there was one each (one set for every look) become the
 *  overrides of the look then chosen. */
function savedTunes(saved: Record<string, unknown>): Prefs['playerBandsTunes'] {
  const check = (raw: unknown): Partial<BandTune> => {
    const out: Partial<BandTune> = {};
    if (!raw || typeof raw !== 'object') return out;
    const r = raw as Record<string, unknown>;
    for (const [k, range] of Object.entries(TUNE_RANGES) as [Exclude<keyof BandTune, 'direction'>, { min: number; max: number }][]) {
      const v = r[k];
      if (typeof v === 'number' && v >= range.min && v <= range.max) out[k] = v;
    }
    if (r.direction === 'left' || r.direction === 'right') out.direction = r.direction;
    return out;
  };
  const tunes: Prefs['playerBandsTunes'] = {};
  const kept = saved.playerBandsTunes as Record<string, unknown> | undefined;
  if (kept && typeof kept === 'object') for (const look of BAND_LOOK_NAMES) if (kept[look]) tunes[look] = check(kept[look]);
  const look = saved.playerBands as BandLookName;
  if (!kept && BAND_LOOK_NAMES.includes(look)) {
    const old = check({
      speed: saved.playerBandsSpeed,
      window: saved.playerBandsWindow,
      cycles: saved.playerBandsCycles,
      height: saved.playerBandsHeight,
      count: saved.playerBandsCount,
      gap: saved.playerBandsGap,
      direction: saved.playerBandsDirection,
      taper: saved.playerBandsTaper,
      fade: saved.playerBandsFade,
    });
    if (Object.keys(old).length) tunes[look] = old;
  }
  return tunes;
}

export type Theme = 'auto' | 'light' | 'dark';
/** What can stand behind the flows: Cratarr's photos (lib/backdrop names them) and one's own picture. */
export const FLOW_BACKDROPS = ['wall', 'flowers', 'leaf', 'pastel', 'records', 'checks', 'mosaic', 'planks', 'amplifier', 'agave', 'rack', 'speakers', 'radios', 'stickers', 'textile', 'succulents', 'own'] as const;
export type FlowBackdrop = (typeof FLOW_BACKDROPS)[number];

export type Prefs = {
  theme: Theme;
  /** What Discover shows (the picker beside back, lib/DiscoverPicker; the owner, 2026-10-29): Fresh, Alike, Gaps or New
   *  artists, or one of the account's Spotify lists (decision 0073, lib/spotify; the Library's before). The Library is
   *  its records alone (the owner, 2026-10-29: its Artists and Tracks went). */
  discoverMode: DiscoverMode;
  /** This device's animations (Display, the owner 2026-10-25): off, as the system's reduced motion (lib/motion). */
  animations: boolean;
  /** Panels' breadcrumbs whole, scrolled to where one is, rather than cut short (lib/Crumbs; a tap on a cut title, the
   *  owner 2026-10-25: "global on the device"); whole by default (the owner, the same day). */
  crumbsWhole: boolean;
  /** The mode switch (Cratarr | Local, decision 0052) in the bar (the phone's) or the sidebar; off, at the top of
   *  Settings (the owner, 2026-10-25: off by default). */
  modeSwitchInBar: boolean;
  /** Where the bar of pages runs below 1024 px (Display, the owner 2026-10-25): along the top or the bottom. */
  navAt: 'top' | 'bottom';
  /** The bar of pages slides away while scrolling down (and back up); off, it always shows. */
  navHides: boolean;
  /** The bar of pages slides away while a library item (a playlist, an album, an artist, a crate) is open: on by
   *  default, as such a panel has long covered it (the owner, 2026-10-25). */
  navHidesOnItem: boolean;
  /** A panel's action bar docks at the top when scrolled to (on by default); in such a panel the bar of pages slides
   *  away scrolling down, as with navHides. Off, the action bar scrolls away with the rest. */
  stickyActions: boolean;
  /** Where the mini-player floats below 1024 px: along the bottom, or the top (under the bar there). */
  miniAt: 'top' | 'bottom';
  /** Where back (or ✕), Home and the title float below 1024 px, next to the mini-player on that edge. */
  navControlsAt: 'top' | 'bottom';
  /** Where the pill (the search, the filter, View, the page's own button) floats below 1024 px. */
  contentControlsAt: 'top' | 'bottom';
  /** Index into the playlist chip widths (Playlists.svelte). */
  zoom: number;
  /** Index into a menu's playlists' widths (LIST_WIDTHS, Add to playlist): one column by default. */
  listZoom: number;
  /** A menu's playlists' order (its header, LIST_ORDERS), either way round. */
  listOrder: PlaylistOrder;
  listDown: boolean;
  /** The full player's row under the title (the owner, 2026-10-19): shown at all, and the facts it shows beside its info
   *  button, in the user's order (lib/trackFacts; none at first). */
  playerRow: boolean;
  /** The full-screen player's bands, landscape (lib/BandGlass): off, or how they are drawn. */
  playerBands: 'off' | BandLookName;
  /** Each look's settings as the user changed them from its defaults (lib/bandTune: TUNE_DEFAULTS), the rest its own
   *  (the owner, 2026-10-24: a default for each look, overrides remembered per look, a reset each). */
  playerBandsTunes: Partial<Record<BandLookName, Partial<BandTune>>>;
  /** How this device draws them (the owner, 2026-10-24: the finest on devices that can, the fastest where they cannot):
   *  SVG, its true blur and blending, or a canvas, one bitmap a frame (59 against 35 frames a second on the tablet). */
  playerBandsEngine: 'svg' | 'canvas';
  playerFacts: FactId[];
  /** A list's profile on top of it (the owner, 2026-10-23): open or folded, for every list at once; its rows switched
   *  on, in the user's order; and its fold line, how many of them stay when it is folded (0: none; Settings › Display ›
   *  AudioMuse). */
  profileOpen: boolean;
  /** Music profiles shown at all (the owner, 2026-10-24: Settings › Display › Content, as the player's extra details):
   *  off, no list or artist shows one. */
  profile: boolean;
  profileRows: ProfileRow[];
  /** Home's sections switched on, in the user's order (Settings › Home; the owner, 2026-10-24). */
  homeParts: HomePart[];
  /** Home's header greets by name ("Good evening, Ann", then "Best for now") in place of the moment's own name. */
  homeGreeting: boolean;
  /** The greeting's name with a capital first letter, for an account named in lower case (the owner, 2026-10-25). */
  /** One's own header picture (Appearance) replaces Home's weather sky too, besides being every flow's backdrop. */
  flowPictureOnHome: boolean;
  /** Behind every flow (the owner, 2026-10-26): one of Cratarr's photos (public/backdrops), or one's own picture. */
  flowBackdrop: FlowBackdrop;
  /** The playing cover's colour (the owner, 2026-10-27: the user's to choose, each place apart): on the floating
   *  controls' glass, on the mini-player, and as the full player's blurred background. Off: Cratarr's own. */
  coverColourGlass: boolean;
  coverColourMini: boolean;
  coverColourPlayer: boolean;
  /** Shuffle (the owner, 2026-10-27): the ticked ones take turns behind the flows, in place of the one chosen. */
  flowShuffle: boolean;
  /** The ones ticked for Shuffle. */
  flowShuffled: FlowBackdrop[];
  /** When Shuffle takes another (the owner, 2026-10-27: the user's to say): each flow that opens, each flow its own
   *  (always the same one for the same flow), each start, or every few minutes. */
  flowShuffleWhen: 'flow' | 'seeded' | 'start' | 'timed';
  /** The visuals open by themselves (0.10; the owner, 2026-10-30): while music plays and the screen is left alone
   *  this long (ms), full screen, a touch closing them; starting from a snapshot of the library's or as last seen; as it
   *  is, or a new look (the dice's) each track. This device's. */
  visualsAuto: boolean;
  visualsAutoAfter: number;
  visualsAutoStart: string;
  visualsAutoRun: 'fixed' | 'track';
  /** One's own crates on Home (Settings › Home, 0.9.3; the owner, 2026-10-25): each a section of its own, a shelf of its
   *  albums, ordered and switched in Home's sections list. */
  homeCrates: boolean;
  profileFoldAt: number;
  /** Levelling (decision 0035): this browser's player plays every track about as loud as the next. */
  /** This browser's name as a Play On player, while a page plays here (lib/playOnTarget); '' until it has one. */
  browserName: string;
  levelling: boolean;
  /** Levelling a track not measured yet (the owner, 2026-10-26): turned down as a typical track is, or left as it is. */
  levelUnknown: 'typical' | 'untouched';
  /** The player's waveform and the levelling (the owner, 2026-10-27, Level the volume › Waveform): as on file (none of
   *  it), the difference (the track as made, the playing colour only as high as heard), or the output (the track as
   *  heard, lowered whole); the Level button's long press goes through the three in turn. This page's showing only. */
  waveLevel: 'file' | 'difference' | 'output';
  /** The full player's row under the title as a tape deck (lib/TapeDeck; the owner, 2026-10-27): never, during an
   *  effect (a Start and stop ramp, a rewind), or always. This page's showing only. */
  tapeDeck: 'off' | 'effects' | 'always';
  /** The installed page's downloads (0.9.8, lib/offline): smaller copies (Opus, or AAC where Opus is not played) or
   *  the originals (the owner, 2026-10-26: Opus in the options too). */
  offlineQuality: 'smaller' | 'original';
  /** How play and pause start and stop (Settings › This device; the owner, 2026-10-25): at once, as a turntable (the
   *  speed, and the pitch with it) or fading; over `startStopMs`; for next and previous too (`startStopSkips`). `rewind`:
   *  Tape rewind: Previous restarting a track plays it backwards to its start, as a tape (the server's clip, as long and
   *  as fast as its tunings say). The app keeps its own. */
  startStop: StartStop;
  /** What the player's four swipes do (Settings › Player; the owner, 2026-10-26): up and down, on its left half and on
   *  its right. */
  playerSwipes: PlayerSwipes;
  /** A phone's player streams lossless, not Opus (the owner, 2026-10-24: Opus on phones unless asked). */
  phoneLossless: boolean;
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
  insightsCollection: 'overview' | 'genres' | 'artists' | 'tracks' | 'discover';
  /** What Genres' and Artists' bars count: all the tracks, or those in a playlist (Insights' panel). */
  insightsCount: 'tracks' | 'listed';
  /** How Collection's long lists are sorted (Genres, Artists, Playlists' most shared): the most first, or A–Z. */
  insightsSort: 'most' | 'az';
  /** What Collection's Tracks counts: tracks in a playlist, or plays. */
  tracksCount: 'listed' | 'plays';
  /** Insights' period, one for all of it (the owner, 2026-10-23): the past week, month or year, or all. Listening
   *  over it; Collection the albums that came in it; not the map. */
  insightsPeriod: 'week' | 'month' | 'year' | 'all';
  /** Listening's own sections (its second row): what was heard, or the taste it makes. */
  listeningView: 'history' | 'profile';
  /** What Profile is built from (its panel): each source on or off. */
  profileSources: { favourites: boolean; playlists: boolean; plays: boolean; skipped: boolean; excluded: boolean };
  signals: { tracks: boolean; genre: boolean; artists: boolean };
  /** Genre territories on Insights' Map (the owner, 2026-10-24: a switch, off until turned on). */
  mapTerritories: boolean;
  /** The genres the Map's territories show, in the order ticked (their colours follow it); null: the most shared six. */
  mapGenres: string[] | null;
  /** The player's width when it shows beside the page (decision 0018). */
  playerWidth: number;
  /** Wide windows show the player beside the page (true) or the mini-player with the player over the page (false,
   *  after Maximise). */
  sidebar: boolean;
  /** The navigation folded to a strip of icons on a wide window (decision 0042). */
  navFolded: boolean;
  /** The part of the settings last open: Settings opens on it beside its list on a roomy window ('' = the first). */
  settingsPart: string;
  /** The Health problem last open beside its list. */
  healthPart: string;
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
  /** The Playlists page in Quick add (0.9.18; the owner, 2026-10-29): each playlist's circle adds or takes out the
   *  playing track; in Normal the playlists only open. */
  playlistQuickAdd: boolean;
  /** The queue sheet's tracks asked for shown or hidden (0.10), as chosen on this device; null: shown while others
   *  may follow this account (lib/queueRequests). */
  queueRequestsShown: boolean | null;
  crateOrder: CrateOrder;
  crateDown: boolean;
  /** The panel's track lists' order, per kind of list (decision 0060). */
  panelSorts: Partial<Record<PanelKind, ListSort>>;
  /** Each crate as records or tracks, as chosen for it (0.10, lib/crateRecords; none: as its kind starts), and the
   *  records' order. */
  crateViews: Record<string, CrateView>;
  crateRecordSort: { key: RecordOrder; down: boolean };
  /** Every list's filter, per kind of list (a page's id, or a panel kind; decisions 0060 and 0061). */
  filters: Partial<Record<string, ListFilter>>;
};

export type AlbumLayout = 'rows' | 'grid' | 'table' | 'flow';
/** How long the screen is left alone before the visuals open by themselves (ms). */
export const VISUALS_AFTER = [30_000, 60_000, 120_000, 300_000, 600_000];
export type CollectionOrder = 'artist' | 'title' | 'year' | 'added' | 'plays' | 'played' | 'rating' | 'quality' | 'tracks' | 'length';
const COLLECTION_ORDERS: readonly CollectionOrder[] = ['artist', 'title', 'year', 'added', 'plays', 'played', 'rating', 'quality', 'tracks', 'length'];
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
/** Up: the queue, Add to playlist, the track's menu; down: the player closing, the visuals. */
export type SwipeUp = 'queue' | 'playlists' | 'menu';
export type SwipeDown = 'close' | 'visuals';
/** Left and right (on the cover): a track on or back, or a seek. */
export type SwipeSide = 'next' | 'previous' | 'forward' | 'back';
/** A tap, a double tap, a circle (on the cover). */
export type CoverTouch = 'playPause' | 'favourite' | 'album' | 'details';
export type PlayerSwipes = {
  upLeft: SwipeUp;
  upRight: SwipeUp;
  downLeft: SwipeDown;
  downRight: SwipeDown;
  left: SwipeSide;
  right: SwipeSide;
  tap: CoverTouch;
  doubleTap: CoverTouch;
  circle: CoverTouch;
};
/** A seek's step (s). */
export const SEEK_STEP = 10;
export const SWIPES_SIDE: { value: SwipeSide; label: string }[] = [
  { value: 'next', label: 'Next track' },
  { value: 'previous', label: 'Previous track' },
  { value: 'forward', label: `Forward ${SEEK_STEP} s` },
  { value: 'back', label: `Back ${SEEK_STEP} s` },
];
export const COVER_TOUCHES: { value: CoverTouch; label: string }[] = [
  { value: 'playPause', label: 'Play or pause' },
  { value: 'favourite', label: 'Favourite' },
  { value: 'album', label: 'Open the album' },
  { value: 'details', label: 'Details' },
];
export const SWIPES_DEFAULT: PlayerSwipes = {
  upLeft: 'queue',
  upRight: 'queue',
  downLeft: 'close',
  downRight: 'close',
  left: 'next',
  right: 'previous',
  tap: 'playPause',
  doubleTap: 'favourite',
  circle: 'details',
};
export const SWIPES_UP: { value: SwipeUp; label: string }[] = [
  { value: 'queue', label: 'Queue' },
  { value: 'playlists', label: 'Add to playlist' },
  { value: 'menu', label: 'Menu' },
];
export const SWIPES_DOWN: { value: SwipeDown; label: string }[] = [
  { value: 'close', label: 'Close the player' },
  { value: 'visuals', label: 'Visuals' },
];

/** The ramp's bounds (ms): a fifth of a second to three. */
export { MOTION, RAMP_MS, startsOf, startStopOf, TAPE, WIND_CAP_MS, type Motion, type PointMotion, type StartStop, type TapeMech, type Wind } from './startStop';
import { startStopOf, type StartStop } from './startStop';
export type FilterCondition = Condition & { enabled?: boolean; quantifier?: 'any' | 'all' };

/** A list profile's rows (lib/ListProfile), in its order: the server's number keys and the word rows. */
/** Home's sections (Home.svelte), in their first order. */
/** One's own crate as a Home section (0.9.3): `crate:<its id>`. */
export const CRATE_PART = 'crate:';
export type HomePart = 'moment' | 'newest' | 'week' | 'back' | 'explore' | 'past' | 'graves' | 'status' | 'favAlbums' | 'favTracks' | `crate:${string}`;
export const HOME_PARTS: { id: HomePart; label: string }[] = [
  { id: 'moment', label: 'Music for the moment' },
  { id: 'newest', label: 'New for you' },
  { id: 'week', label: 'Sounds like your week' },
  { id: 'back', label: 'Jump back in' },
  // After Jump back in (the owner, 2026-10-24).
  { id: 'explore', label: 'Artists to explore' },
  { id: 'past', label: 'Blast from the past' },
  { id: 'graves', label: 'Grave digging' },
  { id: 'status', label: 'System status' },
  // The standard ones, off until switched on (the owner, 2026-10-24).
  { id: 'favAlbums', label: 'Favourite albums' },
  { id: 'favTracks', label: 'Favourite tracks' },
];
/** The sections that can lead Home (the owner, 2026-10-24): made of albums, so the first is shown as the flow on top. */
export const FLOW_PARTS: HomePart[] = ['moment', 'newest', 'past', 'graves', 'favAlbums'];
/** Off until switched on in Settings › Home. */
const HOME_OFF: HomePart[] = ['favAlbums', 'favTracks'];
/** Home's sections as they come: in the first order, the standard ones off. */
export const homeDefault = () => HOME_PARTS.map((r) => r.id).filter((id) => !HOME_OFF.includes(id));
/** The first order before Artists to explore moved up: kept as it was, it takes the new one. */
const HOME_BEFORE = 'moment,newest,week,back,past,graves,explore,status';
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
  animations: true,
  crumbsWhole: true,
  modeSwitchInBar: false,
  discoverMode: 'fresh',
  navAt: 'top',
  navHides: false,
  navHidesOnItem: true,
  stickyActions: true,
  miniAt: 'bottom',
  navControlsAt: 'top',
  contentControlsAt: 'top',
  zoom: 2,
  listZoom: LIST_WIDTHS.length - 1,
  listOrder: 'playing',
  listDown: false,
  playerRow: true,
  playerBands: 'off',
  playerBandsTunes: {},
  playerBandsEngine: 'canvas',
  playerFacts: [],
  profileOpen: true,
  profile: true,
  profileFoldAt: 0,
  profileRows: PROFILE_ROWS.map((r) => r.id),
  homeParts: homeDefault(),
  // On by default (the owner, 2026-10-25).
  homeGreeting: true,
  flowPictureOnHome: false,
  coverColourGlass: true,
  coverColourMini: true,
  coverColourPlayer: true,
  flowShuffle: false,
  flowShuffled: [...FLOW_BACKDROPS.filter((b) => b !== 'own')],
  flowShuffleWhen: 'flow',
  visualsAuto: false,
  visualsAutoAfter: 120_000,
  visualsAutoStart: 'last',
  visualsAutoRun: 'fixed',
  flowBackdrop: 'wall',
  homeCrates: false,
  levelling: true,
  levelUnknown: 'typical',
  waveLevel: 'difference',
  tapeDeck: 'effects',
  offlineQuality: 'smaller',
  startStop: startStopOf({}),
  playerSwipes: { ...SWIPES_DEFAULT },
  phoneLossless: false,
  browserName: '',
  relativeDates: false,
  source: '',
  hiddenLibraries: [],
  insightsTab: 'collection',
  insightsCollection: 'overview',
  insightsCount: 'tracks',
  insightsSort: 'most',
  tracksCount: 'listed',
  insightsPeriod: 'all',
  listeningView: 'history',
  profileSources: { favourites: true, playlists: true, plays: true, skipped: true, excluded: true },
  signals: { tracks: true, genre: true, artists: true },
  mapTerritories: false,
  mapGenres: null,
  playerWidth: 400,
  sidebar: true,
  navFolded: false,
  settingsPart: '',
  healthPart: '',
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
  playlistQuickAdd: false,
  queueRequestsShown: null,
  crateOrder: 'own',
  crateDown: false,
  panelSorts: {},
  crateViews: {},
  crateRecordSort: { key: 'own', down: false },
  filters: {},
};

function read(): Prefs {
  const p = { ...DEFAULTS, signals: { ...DEFAULTS.signals }, pinned: [] as string[], hiddenLibraries: [] as string[], panelSorts: {}, crateViews: {}, filters: {} } as Prefs;
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (saved && typeof saved === 'object') {
      if (['auto', 'light', 'dark'].includes(saved.theme)) p.theme = saved.theme;
      if (typeof saved.animations === 'boolean') p.animations = saved.animations;
      if (typeof saved.crumbsWhole === 'boolean') p.crumbsWhole = saved.crumbsWhole;
      if (typeof saved.modeSwitchInBar === 'boolean') p.modeSwitchInBar = saved.modeSwitchInBar;
      if (isDiscoverMode(saved.discoverMode)) p.discoverMode = saved.discoverMode;
      // A Spotify list left on in the Library before it moved: on in Discover.
      else if (isSpotifyView(saved.libraryView)) p.discoverMode = saved.libraryView;
      if (saved.navAt === 'top' || saved.navAt === 'bottom') p.navAt = saved.navAt;
      if (typeof saved.navHides === 'boolean') p.navHides = saved.navHides;
      if (typeof saved.navHidesOnItem === 'boolean') p.navHidesOnItem = saved.navHidesOnItem;
      if (typeof saved.stickyActions === 'boolean') p.stickyActions = saved.stickyActions;
      for (const k of ['miniAt', 'navControlsAt', 'contentControlsAt'] as const) if (saved[k] === 'top' || saved[k] === 'bottom') p[k] = saved[k];
      if (Number.isInteger(saved.zoom) && saved.zoom >= 0 && saved.zoom < ZOOM_STEPS) p.zoom = saved.zoom;
      if (Number.isInteger(saved.listZoom) && saved.listZoom >= 0 && saved.listZoom < LIST_WIDTHS.length) p.listZoom = saved.listZoom;
      if (typeof saved.filterOpen === 'boolean') p.filterOpen = saved.filterOpen;
      if (typeof saved.relatedFilterOpen === 'boolean') p.relatedFilterOpen = saved.relatedFilterOpen;
      if (['rows', 'grid', 'table', 'flow'].includes(saved.albumLayout)) p.albumLayout = saved.albumLayout;
      if (Number.isInteger(saved.albumZoom) && saved.albumZoom >= 0 && saved.albumZoom < ZOOM_STEPS) p.albumZoom = saved.albumZoom;
      if (COLLECTION_ORDERS.includes(saved.libraryOrder)) p.libraryOrder = saved.libraryOrder;
      if (COLLECTION_ORDERS.includes(saved.downloadsOrder)) p.downloadsOrder = saved.downloadsOrder;
      if (PLAYLIST_ORDERS.includes(saved.playlistOrder)) p.playlistOrder = saved.playlistOrder;
      if (saved.playlistLayout === 'chips' || saved.playlistLayout === 'table') p.playlistLayout = saved.playlistLayout;
      if (typeof saved.playlistQuickAdd === 'boolean') p.playlistQuickAdd = saved.playlistQuickAdd;
      if (typeof saved.queueRequestsShown === 'boolean') p.queueRequestsShown = saved.queueRequestsShown;
      if (PLAYLIST_ORDERS.includes(saved.listOrder) && saved.listOrder !== 'missing') p.listOrder = saved.listOrder;
      if (typeof saved.listDown === 'boolean') p.listDown = saved.listDown;
      // Before the list header: "✓ first" was the playlists' only other order.
      else if (saved.membersFirst === true) p.playlistOrder = 'playing';
      if (CRATE_ORDERS.includes(saved.crateOrder)) p.crateOrder = saved.crateOrder;
      for (const k of ['libraryDown', 'downloadsDown', 'playlistDown', 'crateDown'] as const) {
        if (typeof saved[k] === 'boolean') p[k] = saved[k];
      }
      if (typeof saved.playerRow === 'boolean') p.playerRow = saved.playerRow;
      if (['off', 'aurora', 'sirius', 'hairlines', 'meter'].includes(saved.playerBands)) p.playerBands = saved.playerBands;
      p.playerBandsTunes = savedTunes(saved);
      if (saved.playerBandsEngine === 'svg' || saved.playerBandsEngine === 'canvas') p.playerBandsEngine = saved.playerBandsEngine;
      if (Array.isArray(saved.playerFacts)) p.playerFacts = saved.playerFacts.filter((x: unknown): x is FactId => FACTS.some((f) => f.id === x));
      if (typeof saved.profileOpen === 'boolean') p.profileOpen = saved.profileOpen;
      if (typeof saved.profile === 'boolean') p.profile = saved.profile;
      if (typeof saved.homeGreeting === 'boolean') p.homeGreeting = saved.homeGreeting;
      if (typeof saved.flowPictureOnHome === 'boolean') p.flowPictureOnHome = saved.flowPictureOnHome;
      // Renamed on 2026-10-26 (the owner): Clock is Mosaic, Yellow is Mellow; Mellow is Pastel since 2026-10-27 (another
      // photo); the ones taken away (Grey, Lamp) fall back to the default.
      const backdrop = ({ clock: 'mosaic', yellow: 'pastel', mellow: 'pastel' } as Record<string, string>)[saved.flowBackdrop as string] ?? saved.flowBackdrop;
      if (FLOW_BACKDROPS.includes(backdrop as FlowBackdrop)) p.flowBackdrop = backdrop as FlowBackdrop;
      if (typeof saved.flowShuffle === 'boolean') p.flowShuffle = saved.flowShuffle;
      for (const k of ['coverColourGlass', 'coverColourMini', 'coverColourPlayer'] as const) if (typeof saved[k] === 'boolean') p[k] = saved[k];
      if (Array.isArray(saved.flowShuffled)) {
        const ticked = saved.flowShuffled.filter((b: unknown): b is FlowBackdrop => FLOW_BACKDROPS.includes(b as FlowBackdrop));
        if (ticked.length) p.flowShuffled = ticked;
      }
      if (['flow', 'seeded', 'start', 'timed'].includes(saved.flowShuffleWhen)) p.flowShuffleWhen = saved.flowShuffleWhen;
      if (typeof saved.visualsAuto === 'boolean') p.visualsAuto = saved.visualsAuto;
      if (VISUALS_AFTER.includes(saved.visualsAutoAfter)) p.visualsAutoAfter = saved.visualsAutoAfter;
      if (typeof saved.visualsAutoStart === 'string' && saved.visualsAutoStart) p.visualsAutoStart = saved.visualsAutoStart;
      if (saved.visualsAutoRun === 'fixed' || saved.visualsAutoRun === 'track') p.visualsAutoRun = saved.visualsAutoRun;
      if (typeof saved.homeCrates === 'boolean') p.homeCrates = saved.homeCrates;
      if (Array.isArray(saved.profileRows)) p.profileRows = saved.profileRows.filter((x: unknown): x is ProfileRow => PROFILE_ROWS.some((r) => r.id === x));
      if (Array.isArray(saved.homeParts) && saved.homeParts.join(',') !== HOME_BEFORE)
        p.homeParts = saved.homeParts.filter(
          (x: unknown): x is HomePart => HOME_PARTS.some((r) => r.id === x) || (typeof x === 'string' && x.startsWith(CRATE_PART) && x.length > CRATE_PART.length),
        );
      if (Number.isInteger(saved.profileFoldAt) && saved.profileFoldAt >= 0) {
        // Rows no longer in the profile (Artists, 2026-10-23) that were above the fold line no longer count there.
        const gone = Array.isArray(saved.profileRows) ? saved.profileRows.slice(0, saved.profileFoldAt).filter((x: unknown) => !PROFILE_ROWS.some((r) => r.id === x)).length : 0;
        p.profileFoldAt = Math.min(saved.profileFoldAt - gone, p.profileRows.length);
      }
      if (typeof saved.levelling === 'boolean') p.levelling = saved.levelling;
      if (saved.levelUnknown === 'typical' || saved.levelUnknown === 'untouched') p.levelUnknown = saved.levelUnknown;
      if (saved.waveLevel === 'file' || saved.waveLevel === 'difference' || saved.waveLevel === 'output') p.waveLevel = saved.waveLevel;
      else if ((saved as { levelInWave?: unknown }).levelInWave === false) p.waveLevel = 'file';
      if (saved.tapeDeck === 'off' || saved.tapeDeck === 'effects' || saved.tapeDeck === 'always') p.tapeDeck = saved.tapeDeck;
      // Its first form, a switch: on was during effects.
      else if (typeof saved.tapeDeck === 'boolean') p.tapeDeck = saved.tapeDeck ? 'effects' : 'off';
      if (saved.offlineQuality === 'smaller' || saved.offlineQuality === 'original') p.offlineQuality = saved.offlineQuality;
      if (saved.playerSwipes && typeof saved.playerSwipes === 'object') {
        const w = saved.playerSwipes;
        const up = (v: unknown, d: SwipeUp): SwipeUp => (SWIPES_UP.some((o) => o.value === v) ? (v as SwipeUp) : d);
        const down = (v: unknown, d: SwipeDown): SwipeDown => (SWIPES_DOWN.some((o) => o.value === v) ? (v as SwipeDown) : d);
        const one = <T extends string>(v: unknown, of: { value: T }[], d: T): T => (of.some((o) => o.value === v) ? (v as T) : d);
  p.playerSwipes = {
    upLeft: up(w.upLeft, 'queue'),
    upRight: up(w.upRight, 'queue'),
    downLeft: down(w.downLeft, 'close'),
    downRight: down(w.downRight, 'close'),
    left: one(w.left, SWIPES_SIDE, 'next'),
    right: one(w.right, SWIPES_SIDE, 'previous'),
    tap: one(w.tap, COVER_TOUCHES, 'playPause'),
    doubleTap: one(w.doubleTap, COVER_TOUCHES, 'favourite'),
    circle: one(w.circle, COVER_TOUCHES, 'details'),
  };
      }
      if (saved.startStop && typeof saved.startStop === 'object') {
        p.startStop = startStopOf(saved.startStop);
      }
      if (typeof saved.phoneLossless === 'boolean') p.phoneLossless = saved.phoneLossless;
      if (typeof saved.browserName === 'string') p.browserName = saved.browserName;
      if (typeof saved.relativeDates === 'boolean') p.relativeDates = saved.relativeDates;
      if (typeof saved.source === 'string') p.source = saved.source;
      if (Number.isFinite(saved.playerWidth) && saved.playerWidth >= 275) p.playerWidth = saved.playerWidth;
      if (typeof saved.sidebar === 'boolean') p.sidebar = saved.sidebar;
      if (typeof saved.navFolded === 'boolean') p.navFolded = saved.navFolded;
      if (typeof saved.settingsPart === 'string') p.settingsPart = saved.settingsPart;
      if (typeof saved.healthPart === 'string') p.healthPart = saved.healthPart;
      if (saved.columns && typeof saved.columns === 'object')
        for (const [k, v] of Object.entries(saved.columns))
          if (Array.isArray(v)) p.columns[k] = v.filter((x: unknown): x is string => typeof x === 'string');
      if (saved.output === 'remote' || saved.output === 'local' || (typeof saved.output === 'string' && saved.output.startsWith('player:')))
        p.output = saved.output;
      if (Array.isArray(saved.pinned)) p.pinned = saved.pinned.filter((x: unknown): x is string => typeof x === 'string');
      if (Array.isArray(saved.hiddenLibraries)) p.hiddenLibraries = saved.hiddenLibraries.filter((x: unknown): x is string => typeof x === 'string');
      if (saved.crateViews && typeof saved.crateViews === 'object')
        for (const [k, v] of Object.entries(saved.crateViews)) if (v === 'records' || v === 'tracks') p.crateViews[k] = v;
      if (RECORD_ORDERS.some((o) => o.value === saved.crateRecordSort?.key) && typeof saved.crateRecordSort?.down === 'boolean')
        p.crateRecordSort = { key: saved.crateRecordSort.key, down: saved.crateRecordSort.down };
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
      // Playlists became Tracks (the owner, 2026-10-23: it is about tracks).
      if (saved.insightsCollection === 'playlists') p.insightsCollection = 'tracks';
      else if (['overview', 'genres', 'artists', 'tracks', 'discover'].includes(saved.insightsCollection)) p.insightsCollection = saved.insightsCollection;
      if (saved.insightsCount === 'tracks' || saved.insightsCount === 'listed') p.insightsCount = saved.insightsCount;
      if (saved.insightsSort === 'most' || saved.insightsSort === 'az') p.insightsSort = saved.insightsSort;
      if (saved.tracksCount === 'listed' || saved.tracksCount === 'plays') p.tracksCount = saved.tracksCount;
      if (['week', 'month', 'year', 'all'].includes(saved.insightsPeriod)) p.insightsPeriod = saved.insightsPeriod;
      if (saved.listeningView === 'history' || saved.listeningView === 'profile') p.listeningView = saved.listeningView;
      if (saved.profileSources && typeof saved.profileSources === 'object')
        for (const k of Object.keys(p.profileSources) as (keyof typeof p.profileSources)[]) if (typeof saved.profileSources[k] === 'boolean') p.profileSources[k] = saved.profileSources[k];
      if (typeof saved.mapTerritories === 'boolean') p.mapTerritories = saved.mapTerritories;
      if (Array.isArray(saved.mapGenres) && saved.mapGenres.every((g: unknown) => typeof g === 'string')) p.mapGenres = saved.mapGenres;
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
    if (prefs.animations) document.documentElement.removeAttribute('data-motion');
    else document.documentElement.dataset.motion = 'off';
  });
});
