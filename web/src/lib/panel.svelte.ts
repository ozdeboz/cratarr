// The panel's subject (decisions 0023, 0025): the player, or what is viewed lying under it: a selection of one or more
// playlists in tabs (Tracks, Info, Export, Repair, Sync), a crate (`crate:…`, its builder `crate-new`, `crate-edit:…`),
// or an album (`album:<server>/<id>`).
// It is in the address as `?view=…` (repeated for each playlist) and `&tab=…` on whichever page shows (Back closes
// it, a reload brings it back), and it stays viewed across pages: the navigation keeps it in its links (`keepViewed`).

import { leaveOnArrival } from './navStep.svelte';

export type Tab = 'tracks' | 'info' | 'export' | 'repair' | 'sync';
export const TABS: { tab: Tab; label: string }[] = [
  { tab: 'tracks', label: 'Tracks' },
  { tab: 'info', label: 'Info' },
  { tab: 'export', label: 'Export' },
  { tab: 'repair', label: 'Repair' },
  { tab: 'sync', label: 'Sync' },
];
const isTab = (t: string | null): t is Tab => TABS.some((x) => x.tab === t);

/** An album in the address, and back. */
const ALBUM = 'album:';
export const albumSubject = (source: string, id: string) => `${ALBUM}${source}/${id}`;
export function albumOf(viewed: string | null): { source: string; id: string } | null {
  if (!viewed?.startsWith(ALBUM)) return null;
  const rest = viewed.slice(ALBUM.length);
  const at = rest.lastIndexOf('/');
  return at > 0 && at < rest.length - 1 ? { source: rest.slice(0, at), id: rest.slice(at + 1) } : null;
}

/** An artist in the address (`artist:<server>/<name>`, decided 2026-10-16), by name as the albums credit them; and all
 *  their tracks, most played first (`artist-tracks:`, their Top recommendations' ›). */
const ARTIST = 'artist:';
const ARTIST_TRACKS = 'artist-tracks:';
export const artistSubject = (source: string, name: string, tracks = false) => `${tracks ? ARTIST_TRACKS : ARTIST}${source}/${name}`;
export function artistOf(viewed: string | null): { source: string; name: string; tracks: boolean } | null {
  const tracks = !!viewed?.startsWith(ARTIST_TRACKS);
  if (!tracks && !viewed?.startsWith(ARTIST)) return null;
  const rest = viewed!.slice((tracks ? ARTIST_TRACKS : ARTIST).length);
  // A server's id has no slash; a name may.
  const at = rest.indexOf('/');
  return at > 0 && at < rest.length - 1 ? { source: rest.slice(0, at), name: rest.slice(at + 1), tracks } : null;
}

/** A crate, its builder, an album or an artist: a subject of one, without tabs (decision 0025). */
/** A playlist being made: the panel's edit mode, with no playlist yet (New playlist, as `crate-new` is New crate). */
export const NEW_PLAYLIST = 'playlist-new';

/** A part of the settings (`settings:libraries`), opened from the Settings page's list (lib/SettingsPanel); a thing's
 *  own page in it after a slash (`settings:connections/jellyfin`, the settings' North Star, 2026-10-19). */
export const SETTINGS = 'settings:';
export const settingsOf = (viewed: string | null) => (viewed?.startsWith(SETTINGS) ? viewed.slice(SETTINGS.length).split('/')[0] : null);
/** The thing whose own page shows in a part of the settings, if one does. */
export function settingsThing(viewed: string | null): string | null {
  if (!viewed?.startsWith(SETTINGS)) return null;
  const at = viewed.indexOf('/', SETTINGS.length);
  return at < 0 ? null : decodeURIComponent(viewed.slice(at + 1));
}
/** A thing's own page in a part of the settings (a server, a library, a Kodi, a user): a step of its own, which Back
 *  (or the page's back button, backFromThing) returns from. */
export function openThing(part: string, thing: string) {
  openStep(`${SETTINGS}${part}/${encodeURIComponent(thing)}`);
}
/** A page one step deeper in the panel (a part's thing, a problem's item): a step of its own, which back returns from
 *  (backFromStep). */
export function openStep(subject: string) {
  panel.playerTop = false;
  history.pushState({ panel: depth() + 1, thing: true }, '', address([subject], 'tracks'));
  read();
}
/** Back from a thing's page to its part: the step it was opened as, or else the part in its place (opened by its
 *  address, or the thing gone). Once while a step back is under way (a removal goes back, and so does its page on
 *  finding the thing gone). */
let goingBack = false;
export function backFromThing(part: string) {
  backFromStep(`${SETTINGS}${part}`);
}
/** Back from a step deeper to `subject`: the step it was opened as, or else `subject` in its place. */
export function backFromStep(subject: string) {
  if (goingBack) return;
  if (history.state?.thing) {
    goingBack = true;
    addEventListener('popstate', () => (goingBack = false), { once: true });
    return history.back();
  }
  replace(address([subject], 'tracks'));
  read();
}

/** A problem of Health (`health:<its key>`), opened from the Health page's cards (lib/HealthPanel), as Settings' parts. */
export const HEALTH = 'health:';
export const healthOf = (viewed: string | null) => (viewed?.startsWith(HEALTH) ? viewed.slice(HEALTH.length).split('/')[0] : null);
/** The item of the problem whose own page shows (`health:<problem>/<item>`, the third level), if one does. */
export function healthItemOf(viewed: string | null): number | null {
  if (!viewed?.startsWith(HEALTH)) return null;
  const at = viewed.indexOf('/', HEALTH.length);
  return at < 0 ? null : Number(viewed.slice(at + 1));
}

/** A view of Insights (`insights:history`, lib/InsightsPanel), opened from its page's list, as a part of the settings
 *  is (the owner, 2026-10-24). */
export const INSIGHTS = 'insights:';
export const insightsOf = (viewed: string | null) => (viewed?.startsWith(INSIGHTS) ? viewed.slice(INSIGHTS.length) : null);

/** A list of Requests (`requests:mine`, lib/RequestsPanel), opened from its page's index, as Insights' views are (the
 *  owner, 2026-10-24). */
export const REQUESTS = 'requests:';
export const requestsOf = (viewed: string | null) => (viewed?.startsWith(REQUESTS) ? viewed.slice(REQUESTS.length) : null);
/** A list of Downloads (`downloads:synced`, lib/DownloadsPanel), opened from its page's index, as Requests' are (the
 *  owner, 2026-10-24: these pages have their own panels, never the content panel). */
export const DOWNLOADS = 'downloads:';
export const downloadsOf = (viewed: string | null) => (viewed?.startsWith(DOWNLOADS) ? viewed.slice(DOWNLOADS.length) : null);

export const isSingleSubject = (viewed: string | null) =>
  !!viewed &&
  (viewed.startsWith(INSIGHTS) ||
    viewed.startsWith(REQUESTS) ||
    viewed.startsWith(DOWNLOADS) || viewed.startsWith(HEALTH) || viewed.startsWith('crate:') || viewed === 'crate-new' || viewed.startsWith('crate-edit:') || viewed === NEW_PLAYLIST || viewed.startsWith(SETTINGS) || viewed.startsWith(ALBUM) || artistOf(viewed) !== null);

export const panel = $state({
  /** What is viewed: the (first) playlist's id, a crate's or an album's key; null when the panel shows the player only. */
  viewed: null as string | null,
  /** What the panel shows: `viewed` once it is ready there (lib/Hold), which a page marks as open. */
  shown: null as string | null,
  /** The playlists selected, in the order ticked; empty for a crate. */
  selection: [] as string[],
  tab: 'tracks' as Tab,
  /** The player shows over what is viewed (the mini-player was tapped); folding the player shows it again. */
  playerTop: false,
  /** The page keeps the panel beside its list (Settings, Health, Insights, Downloads on a roomy window): its place
   *  cannot change there, so nothing offers to (PanelPlace). */
  split: false,
  /** What it showed when the page was entered (openedHere). */
  arrived: null as string | null,
  /** Bumped when the viewed playlist is viewed again: its view scrolls back to the top. */
  again: 0,
});

const query = () => new URLSearchParams(location.hash.split('?')[1] ?? '');
const base = () => location.hash.split('?')[0] || '#/';
/** What of the address is the page's own, kept as the panel opens and closes over it (the Library's composer, 0.10). */
const PAGE_KEYS = ['composer'];
function pageQuery() {
  const q = query();
  const out = new URLSearchParams();
  for (const k of PAGE_KEYS) {
    const v = q.get(k);
    if (v) out.set(k, v);
  }
  return out;
}
/** The page's address, its own part kept. */
const pageAddress = () => {
  const q = pageQuery().toString();
  return q ? `${base()}?${q}` : base();
};
function address(names: string[], tab: Tab) {
  const q = pageQuery();
  for (const n of names) q.append('view', n);
  if (tab !== 'tracks') q.set('tab', tab);
  return `${base()}?${q}`;
}

// The history entries the panel added in this visit carry how many there are: closing goes Back over all of them, so
// the history stays tidy (viewing adds one; another view from the menu adds one more).
const depth = (): number => (history.state && typeof history.state.panel === 'number' ? history.state.panel : 0);
/** How many of the panel's own views (and tab steps) this visit went through to here: back goes to the one before. */
export const panelDepth = depth;
function push(url: string) {
  history.pushState({ panel: depth() + 1 }, '', url);
}
function replace(url: string) {
  history.replaceState(history.state, '', url);
}

/** What follows the address with the panel, in the same step (App's page): a page and its panel change together, never
 *  one drawn without the other (the owner, 2026-10-25: Home from an artist's panel showed the Library's filter and
 *  sort for a moment, the panel closed while the page was still the Library). */
const alongside = new Set<() => void>();
export function followAddress(f: () => void) {
  alongside.add(f);
  return () => void alongside.delete(f);
}

function read() {
  for (const f of alongside) f();
  const q = query();
  const viewed = q.getAll('view');
  const first = viewed[0] ?? null;
  if (first !== panel.viewed || viewed.join('\n') !== panel.selection.join('\n')) {
    panel.playerTop = false;
  }
  panel.viewed = first;
  if (first === null) panel.shown = null;
  panel.selection = first && !isSingleSubject(first) ? viewed : [];
  const tab = q.get('tab');
  panel.tab = panel.selection.length && isTab(tab) ? tab : 'tracks';
}

read();
addEventListener('hashchange', read);
addEventListener('popstate', read);

/** Views playlists (one, or a selection), a crate or an album in the panel, on `tab`. What is already viewed is replaced,
 *  without a step in the history (no stack: ✕ and Back then close it rather than going to the one before). */
export function viewSelection(names: string[], tab: Tab = 'tracks') {
  if (!names.length) return closePlaylist();
  const t = isSingleSubject(names[0]) ? 'tracks' : tab;
  panel.playerTop = false;
  if (names.join('\n') === (panel.selection.length ? panel.selection : [panel.viewed]).join('\n') && t === panel.tab) {
    panel.again++;
    return;
  }
  if (panel.viewed) replace(address(names, t));
  else push(address(names, t));
  read();
}

/** Views one playlist (or a crate, or an album) on its Tracks. */
export const viewPlaylist = (name: string) => viewSelection([name]);

/** Views a subject without a step in the history, even when nothing was viewed: what a page shows by itself (Settings
 *  opening a part beside its list), which Back does not close. */
export function viewInPlace(name: string) {
  panel.playerTop = false;
  replace(address([name], 'tracks'));
  read();
}

/** Views a subject over the one viewed, going deeper (an album's artist, an artist's album, as Symfonium's): a step of
 *  its own, which back returns from. With nothing viewed, as viewPlaylist. */
export function viewOver(name: string) {
  if (!panel.viewed) return viewPlaylist(name);
  panel.playerTop = false;
  if (name === panel.viewed && !panel.selection.length) {
    panel.again++;
    return;
  }
  push(address([name], 'tracks'));
  read();
}

/** One of the playlists' other views (Info, Export, Repair, Sync: from their menu, decided 2026-10-16). Over their
 *  tracks it is a step of its own, which back returns from (backToTracks); playlists not viewed yet open on it. */
export function openTab(names: string[], tab: Tab) {
  if (names.join('\n') !== panel.selection.join('\n')) return viewSelection(names, tab);
  if (tab === panel.tab) return;
  history.pushState({ panel: depth() + 1, tab: true }, '', address(panel.selection, tab));
  read();
}

/** Back from another view to the tracks: the step it was opened as, or else the tracks in its place. */
export function backToTracks() {
  if (history.state?.tab) return history.back();
  replace(address(panel.selection, 'tracks'));
  read();
}

/** Closes what is viewed (✕, Esc): the player shows if something plays, else the panel goes. `then` runs once it
 *  is closed (going Back is not immediate). */
/** Whether what the panel shows was opened on this page, not kept from the one before. Settings, Health, Insights and
 *  Downloads fill their panel on arriving (over a playlist kept viewed from the page before) and whenever it is empty,
 *  but never over what is opened while on them: an artist from the player stays (the owner, 2026-10-24: it closed the
 *  player and opened nothing). */
export const openedHere = () => panel.viewed !== null && panel.viewed !== panel.arrived;
/** `subject` is being opened: asked for, while the panel still holds what it showed (lib/Hold). Its cover shows the
 *  loading sign over it, dimmed (the owner, 2026-10-27). */
export const opening = (subject: string) => panel.viewed === subject && panel.shown !== subject;

/** Home, as the navigation's Home is (a step of its own: Back comes back to what was open); from a full-screen panel,
 *  over the navigation (the owner, 2026-10-24). */
export function goHome() {
  panel.playerTop = false;
  location.hash = '#/home';
  // Home's button starts it at its top, from Home itself too (the owner, 2026-10-25); back brings back where it was.
  window.scrollTo(0, 0);
}

export function closePlaylist(then?: () => void) {
  panel.playerTop = false;
  const d = depth();
  if (d > 0) {
    if (then) addEventListener('popstate', () => then(), { once: true });
    history.go(-d);
  } else {
    replace(pageAddress());
    read();
    then?.();
  }
}

/** A breadcrumb's page (lib/Crumbs, 0.9.3): the panel closed, as its ✕ closes it, and that page shown, wherever the
 *  panel was opened from (the owner, 2026-10-25: breadcrumbs as what global search needs, its results opening anywhere). */
export function upToPage(href: string) {
  // Its layer stays over the page until the page gone to shows (lib/navStep), never the page before for a moment.
  if (!location.hash.startsWith(href)) leaveOnArrival({ layer: true }, false, href);
  closePlaylist(() => {
    if (!location.hash.startsWith(href)) location.hash = href;
  });
}

/** A renamed playlist stays viewed under its new name; a deleted one leaves the selection. */
export function playlistRenamed(from: string, to: string) {
  if (!panel.selection.includes(from)) return;
  replace(address(panel.selection.map((x) => (x === from ? to : x)), panel.tab));
  read();
}
export function playlistDeleted(id: string) {
  if (!panel.selection.includes(id)) return;
  const rest = panel.selection.filter((x) => x !== id);
  if (!rest.length) return closePlaylist();
  replace(address(rest, panel.tab));
  read();
}

/** An album that moved (kept or replaced into Music: a new id on the server) stays viewed as itself there. */
export function albumMoved(source: string, from: string, to: string) {
  if (panel.viewed !== albumSubject(source, from)) return;
  replace(address([albumSubject(source, to)], 'tracks'));
  read();
}

/** An album that left Incoming (kept, replaced or deleted) closes. */
export function albumGone(source: string, id: string) {
  if (panel.viewed === albumSubject(source, id)) closePlaylist();
}

/** A page's link that keeps what is viewed viewed. A part of the settings is the Settings page's own, Insights' panel
 *  Insights': they stay there. */
export const keepViewed = (href: string) => {
  const q = location.hash.split('?')[1];
  return panel.viewed && q && !panel.viewed.startsWith(SETTINGS) && !panel.viewed.startsWith(HEALTH) && !panel.viewed.startsWith(INSIGHTS) && !panel.viewed.startsWith(REQUESTS) && !panel.viewed.startsWith(DOWNLOADS)
    ? `${href}?${q}`
    : href;
};
