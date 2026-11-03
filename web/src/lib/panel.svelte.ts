// The panel's subject (decisions 0023, 0025): the player, or what is viewed lying under it: a selection of one or more
// playlists in tabs (Tracks, Export, Repair, Sync; Info is a step of their menu since 2026-11-02), a crate (`crate:…`, its builder `crate-new`, `crate-edit:…`),
// or an album (`album:<server>/<id>`).
// It is in the address as `?view=…` (repeated for each playlist) and `&tab=…` on whichever page shows (a reload brings
// it back), and it stays viewed across pages: the navigation keeps it in its links (`keepViewed`).
// The panel keeps its own history (its trail, below; the owner, 2026-11-02): beside the page, changing what it shows
// leaves the page's history as it is, and the page's Back leaves the panel as it is; full screen, over the page, one
// step of the page's history stands for the trail, so Back (the device's, the browser's) steps back through it.

import { leaveOnArrival } from './navStep.svelte';
import { addressOf, HOME, hrefOf, ownWordsHere, pageHere, wordsIn, type PageId } from './addresses';
import { prefs } from './prefs.svelte';

export type Tab = 'tracks' | 'export' | 'repair' | 'sync';
export const TABS: { tab: Tab; label: string }[] = [
  { tab: 'tracks', label: 'Tracks' },
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
  step([subject], 'tracks', 'thing');
}
/** Back from a thing's page to its part: the step it was opened as, or else the part in its place (opened by its
 *  address, or the thing gone). */
export function backFromThing(part: string) {
  backFromStep(`${SETTINGS}${part}`);
}
/** Back from a step deeper to `subject`: the step it was opened as, or else `subject` in its place. */
export function backFromStep(subject: string) {
  if (trail.at(-1)?.how === 'thing' && trail.length > 1) return backOne();
  swap([subject], 'tracks');
}

/** A problem of Health (`health:<its key>`), opened from the Health page's cards (lib/HealthPanel), as Settings' parts. */
export const HEALTH = 'health:';
export const healthOf = (viewed: string | null) => (viewed?.startsWith(HEALTH) ? viewed.slice(HEALTH.length).split('/')[0] : null);
/** What a Health part shows a step deeper (Changes to files' plan or run, decision 0092), if anything. */
export function healthThing(viewed: string | null): string | null {
  if (!viewed?.startsWith(HEALTH)) return null;
  const at = viewed.indexOf('/', HEALTH.length);
  return at < 0 ? null : decodeURIComponent(viewed.slice(at + 1));
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
  /** The track a link to `subject` came for (a track's Go to album, a track found by global search): its list scrolls
   *  to it and marks it (lib/reveal). `n` counts the links, so the same one again scrolls to it again. */
  mark: null as { subject: string; track: string; n: number } | null,
});

const query = () => wordsIn();
/** The page's address, its own words kept (lib/addresses: the Library's composer, Discover's Spotify playlist…). */
const pageAddress = () => addressOf(pageHere(), ownWordsHere());
/** The page's address with `names` viewed on `tab`, its own words kept. */
function address(names: string[], tab: Tab) {
  const q = ownWordsHere();
  for (const n of names) q.append('view', n);
  if (tab !== 'tracks') q.set('tab', tab);
  return addressOf(pageHere(), q);
}

/** A level of the panel's own history: what it shows, on which tab, and how it was reached (`tab`: another tab over its
 *  tracks; `thing`: a step deeper, a thing's own page; else a view, a page's or the panel's own). */
type Step = { names: string[]; tab: Tab; how: 'view' | 'tab' | 'thing' };
let trail: Step[] = [];
/** The trail as the panels read it: how many levels, and how the one shown was reached. */
const steps = $state({ n: 0, how: 'view' as Step['how'] });
/** How many levels the panel's history has: Back goes to the one before where there is more than one. */
export const panelDepth = () => steps.n;
/** Whether the panel shows beside the page (set by App): its history then is its own, out of the page's. */
const layout = $state({ beside: false });
/** A panel shows beside the page (not over it): the page's search is the one in sight. */
export const panelBeside = () => layout.beside && panel.viewed !== null;
/** A panel covers the page (full screen, over it): the page says nothing about itself meanwhile (decision 0095: a
 *  page's "nothing" over a panel pointed at a list out of sight). */
export const pageCovered = () => !layout.beside && panel.viewed !== null;
export function setBeside(beside: boolean) {
  if (layout.beside === beside) return;
  layout.beside = beside;
  guard();
}

/** What follows the address with the panel, in the same step (App's page): a page and its panel change together, never
 *  one drawn without the other (the owner, 2026-10-25: Home from an artist's panel showed the Library's filter and
 *  sort for a moment, the panel closed while the page was still the Library). */
const alongside = new Set<() => void>();
export function followAddress(f: () => void) {
  alongside.add(f);
  return () => void alongside.delete(f);
}

/** The panel as the trail's last level says, in the address too (no step of the page's history). */
function show() {
  const t = trail.at(-1) ?? null;
  const first = t?.names[0] ?? null;
  if (first !== panel.viewed || (t?.names ?? []).join('\n') !== panel.selection.join('\n')) panel.playerTop = false;
  panel.viewed = first;
  if (first === null) panel.shown = null;
  panel.selection = first && !isSingleSubject(first) ? t!.names : [];
  panel.tab = panel.selection.length ? t!.tab : 'tracks';
  steps.n = trail.length;
  steps.how = t?.how ?? 'view';
  const st = history.state?.panelGuard ? { ...history.state, trail } : history.state;
  history.replaceState(st, '', t ? address(t.names, t.tab) : pageAddress());
  guard();
}

// Full screen, one step of the page's history (`panelGuard`, the trail with it) stands for the trail while it has a
// level: Back takes it, and the panel goes back one level (another guard then for the rest); Back to it from a page
// gone to over the panel (its Home) brings the trail back. Beside the page, none; one left from full
// screen is taken back, quietly.
let dropping: (() => void)[] | null = null;
function guard() {
  const want = !layout.beside && trail.length > 0;
  const has = !!history.state?.panelGuard;
  if (want && !has) {
    at = key();
    history.pushState({ ...(history.state ?? {}), panelGuard: true, trail, pk: at }, '', location.href);
  }
  else if (!want && has && !dropping) {
    dropping = [];
    history.back();
  }
}
/** `then` once the page's history is settled (a guard being taken back is not immediate). */
function settled(then?: () => void) {
  if (!then) return;
  if (dropping) dropping.push(then);
  else then();
}

/** The address as it is, read into the trail: its view the trail's one level (a reload, a link from outside). */
function fromAddress() {
  const q = query();
  const names = q.getAll('view');
  const tab = q.get('tab');
  trail = names.length ? [{ names, tab: isTab(tab) ? tab : 'tracks', how: 'view' }] : [];
}

// Each step of the page's history the panel has seen carries its own key (`pk`); one without is a link followed (an
// address set, `location.hash`; others' marks on a step keep the key, a step another module adds copies it). A link's
// view opens as a step of the panel's history (a page's open, the owner 2026-11-02); none closes the panel (Home's
// button, a page that fills its own panel). A step through the page's history (Back, Forward) goes to its page and
// leaves the panel as it is, but for the guard's own (full screen). Each step is taken once (popstate and hashchange
// both come for one).
let seen = 0;
const key = () => `${Date.now().toString(36)}-${++seen}`;
let at: string | null = null;
function onAddress() {
  const pk = history.state?.pk as string | undefined;
  if (pk && pk === at) return;
  if (!pk) {
    at = key();
    history.replaceState({ ...(history.state ?? {}), pk: at }, '', location.href);
    for (const f of alongside) f();
    const q = query();
    const names = q.getAll('view');
    const tab = isTab(q.get('tab')) ? (q.get('tab') as Tab) : 'tracks';
    const t = trail.at(-1);
    if (!names.length) trail = [];
    else if (!t || names.join('\n') !== t.names.join('\n') || tab !== t.tab) trail = [...trail, { names, tab, how: 'view' }];
    show();
    return;
  }
  at = pk;
  if (dropping) {
    const then = dropping;
    dropping = null;
    show();
    for (const f of then) f();
    return;
  }
  if (!history.state.panelGuard && !layout.beside && trail.length) {
    // Back over the guard: the panel's own Back.
    trail = trail.slice(0, -1);
    show();
    return;
  }
  if (history.state.panelGuard && !layout.beside && !trail.length && Array.isArray(history.state.trail)) trail = history.state.trail;
  for (const f of alongside) f();
  show();
}
addEventListener('popstate', onAddress);
addEventListener('hashchange', onAddress);
at = (history.state?.pk as string | undefined) ?? key();
history.replaceState({ ...(history.state ?? {}), pk: at }, '', location.href);
fromAddress();
show();

/** A level onto the trail. */
function step(names: string[], tab: Tab, how: Step['how'] = 'view') {
  trail = [...trail, { names, tab, how }];
  show();
}
/** The trail's last level in its place (none: the first). */
function swap(names: string[], tab: Tab, how: Step['how'] = 'view') {
  trail = [...trail.slice(0, -1), { names, tab, how }];
  show();
}

/** Views playlists (one, or a selection), a crate or an album in the panel, on `tab`. A selection changing (ticked in
 *  Select) is in its place; a page's open (viewPlaylist) a step of the panel's history (the owner, 2026-11-02: "Each
 *  page open is a step"), which Back returns from. */
export function viewSelection(names: string[], tab: Tab = 'tracks', how: 'swap' | 'step' = 'swap') {
  panel.mark = null;
  if (!names.length) return closePlaylist();
  const t = isSingleSubject(names[0]) ? 'tracks' : tab;
  panel.playerTop = false;
  if (names.join('\n') === (panel.selection.length ? panel.selection : [panel.viewed]).join('\n') && t === panel.tab) {
    panel.again++;
    return;
  }
  if (how === 'step') step(names, t);
  else swap(names, t);
}

/** Views one playlist (or a crate, or an album) on its Tracks: a step of the panel's history. */
export const viewPlaylist = (name: string) => viewSelection([name], 'tracks', 'step');

/** Views a subject in the place of the one shown, even when nothing was viewed: what a page shows by itself (Settings
 *  opening a part beside its list). */
export function viewInPlace(name: string) {
  panel.playerTop = false;
  swap([name], 'tracks');
}

/** Views a subject over the one viewed, going deeper (an album's artist, an artist's album, as Symfonium's): a step of
 *  its own, which back returns from. With nothing viewed, as viewPlaylist. */
export function viewOver(name: string) {
  panel.mark = null;
  if (!panel.viewed) return viewPlaylist(name);
  panel.playerTop = false;
  if (name === panel.viewed && !panel.selection.length) {
    panel.again++;
    return;
  }
  step([name], 'tracks');
}

/** Views `subject` (an album, a playlist, a crate) over the one viewed, for one of its tracks: its list scrolls to the
 *  track and marks it. */
export function viewFor(subject: string, track: string) {
  const n = (panel.mark?.n ?? 0) + 1;
  viewOver(subject);
  panel.mark = { subject, track, n };
}

/** One of the playlists' other views (Info, Export, Repair, Sync: from their menu, decided 2026-10-16). Over their
 *  tracks it is a step of its own, which back returns from (backToTracks); playlists not viewed yet open on it. */
export function openTab(names: string[], tab: Tab) {
  if (names.join('\n') !== panel.selection.join('\n')) return viewSelection(names, tab);
  if (tab === panel.tab) return;
  step(panel.selection, tab, 'tab');
}

/** Back from another view to the tracks: the step it was opened as, or else the tracks in its place. */
export function backToTracks() {
  if (trail.at(-1)?.how === 'tab' && trail.length > 1) return backOne();
  swap(panel.selection, 'tracks');
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
  location.hash = hrefOf(HOME);
  // Home's button starts it at its top, from Home itself too (the owner, 2026-10-25); back brings back where it was.
  window.scrollTo(0, 0);
}

/** A sibling picked (a section's name as the picker of its siblings; the owner, 2026-11-01: "Changing siblings should
 *  be history with back", as the Library's types are): the same level, a step of its own that Back returns from. */
export function viewSibling(name: string) {
  viewOver(name);
}
/** Back one level (the panel's back button, a breadcrumb's level above; the owner, 2026-11-01: one rule for every
 *  panel's breadcrumbs, the levels Back walks through): the step before, or the panel closed where it is the first. */
export function backOne() {
  if (trail.length < 2) return closePlaylist();
  trail = trail.slice(0, -1);
  show();
}
/** What shows was opened a step deeper (openStep): the level above it is the step before. */
export const isStep = () => steps.how === 'thing' && steps.n > 1;

/** Closes the panel, its whole history with it; `then` once the page's history is settled. */
export function closePlaylist(then?: () => void) {
  panel.playerTop = false;
  trail = [];
  show();
  settled(then);
}

/** A breadcrumb's page (lib/Crumbs, 0.9.3): the panel closed, as its ✕ closes it, and that page shown, wherever the
 *  panel was opened from (the owner, 2026-10-25: breadcrumbs as what global search needs, its results opening anywhere). */
export function upToPage(page: PageId) {
  // Its layer stays over the page until the page gone to shows (lib/navStep), never the page before for a moment.
  if (pageHere() !== page) leaveOnArrival({ layer: true }, false, hrefOf(page));
  closePlaylist(() => {
    if (pageHere() !== page) location.hash = hrefOf(page);
  });
}

/** A renamed playlist stays viewed under its new name; a deleted one leaves the selection. */
export function playlistRenamed(from: string, to: string) {
  if (!panel.selection.includes(from)) return;
  trail = trail.map((t) => ({ ...t, names: t.names.map((x) => (x === from ? to : x)) }));
  show();
}
export function playlistDeleted(id: string) {
  if (!panel.selection.includes(id)) return;
  const rest = panel.selection.filter((x) => x !== id);
  if (!rest.length) return backOne();
  swap(rest, panel.tab);
}

/** An album that moved (kept or replaced into Music: a new id on the server) stays viewed as itself there. */
export function albumMoved(source: string, from: string, to: string) {
  if (panel.viewed !== albumSubject(source, from)) return;
  const [was, now] = [albumSubject(source, from), albumSubject(source, to)];
  trail = trail.map((t) => ({ ...t, names: t.names.map((x) => (x === was ? now : x)) }));
  show();
}

/** An album that left Incoming (kept, replaced or deleted) closes. */
/** A record deleted: back one level from it (not every level: what it was opened over stays). */
export function albumGone(source: string, id: string) {
  if (panel.viewed === albumSubject(source, id)) backOne();
}

/** The pages of music, whose panels show the same kinds of things (the owner, 2026-11-02): between them a locked panel
 *  stays, an unlocked one closes. */
const MUSIC_PAGES: readonly PageId[] = ['home', 'crates', 'playlists', 'library', 'discover'];
/** A page's link that keeps what is viewed viewed. A part of the settings is the Settings page's own, Insights' panel
 *  Insights': they stay there. Beside the page, from one page of music to another, only while the panel is locked
 *  (prefs.panelLocked); a link to the page that shows keeps it. */
export const keepViewed = (page: PageId) => {
  const here = pageHere();
  const leaves = layout.beside && !prefs.panelLocked && page !== here && MUSIC_PAGES.includes(page) && MUSIC_PAGES.includes(here);
  const kept = panel.viewed && !leaves && ![SETTINGS, HEALTH, INSIGHTS, REQUESTS, DOWNLOADS].some((p) => panel.viewed!.startsWith(p));
  if (!kept) return hrefOf(page);
  // The panel's words only: the page left keeps its own (the Library's composer is no word of Crates').
  const w = wordsIn();
  return addressOf(page, { view: w.getAll('view'), tab: w.get('tab') });
};
