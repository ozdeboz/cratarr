// The panel's subject (decisions 0023, 0025): the player, or what is viewed lying under it: a selection of one or more
// playlists in tabs (Tracks, Info, Export, Repair, Sync), a crate (`crate:…`, its builder `crate-new`, `crate-edit:…`),
// or an album (`album:<server>/<id>`).
// It is in the address as `?view=…` (repeated for each playlist) and `&tab=…` on whichever page shows (Back closes
// it, a reload brings it back), and it stays viewed across pages: the navigation keeps it in its links (`keepViewed`).

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

/** A crate, its builder or an album: a subject of one, without tabs (decision 0025). */
/** A playlist being made: the panel's edit mode, with no playlist yet (New playlist, as `crate-new` is New crate). */
export const NEW_PLAYLIST = 'playlist-new';

/** A part of the settings (`settings:libraries`), opened from the Settings page's cards (lib/SettingsPanel). */
export const SETTINGS = 'settings:';
export const settingsOf = (viewed: string | null) => (viewed?.startsWith(SETTINGS) ? viewed.slice(SETTINGS.length) : null);

export const isSingleSubject = (viewed: string | null) =>
  !!viewed &&
  (viewed.startsWith('crate:') || viewed === 'crate-new' || viewed.startsWith('crate-edit:') || viewed === NEW_PLAYLIST || viewed.startsWith(SETTINGS) || viewed.startsWith(ALBUM));

export const panel = $state({
  /** What is viewed: the (first) playlist's id, a crate's or an album's key; null when the panel shows the player only. */
  viewed: null as string | null,
  /** The playlists selected, in the order ticked; empty for a crate. */
  selection: [] as string[],
  tab: 'tracks' as Tab,
  /** The player shows over what is viewed (the mini-player was tapped); folding the player shows it again. */
  playerTop: false,
  /** Bumped when the viewed playlist is viewed again: its view scrolls back to the top. */
  again: 0,
  /** The filter's Hide missing tracks, for what is viewed only: off again when something else is (not remembered,
   *  as no filter but the server is, decided 2026-10-16). */
  hideMissing: false,
});

const query = () => new URLSearchParams(location.hash.split('?')[1] ?? '');
const base = () => location.hash.split('?')[0] || '#/';
function address(names: string[], tab: Tab) {
  const q = new URLSearchParams();
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

function read() {
  const q = query();
  const viewed = q.getAll('view');
  const first = viewed[0] ?? null;
  if (first !== panel.viewed || viewed.join('\n') !== panel.selection.join('\n')) {
    panel.playerTop = false;
    panel.hideMissing = false;
  }
  panel.viewed = first;
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
export function closePlaylist(then?: () => void) {
  panel.playerTop = false;
  const d = depth();
  if (d > 0) {
    if (then) addEventListener('popstate', () => then(), { once: true });
    history.go(-d);
  } else {
    replace(base());
    read();
    then?.();
  }
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

/** A page's link that keeps what is viewed viewed. */
export const keepViewed = (href: string) => {
  const q = location.hash.split('?')[1];
  return panel.viewed && q ? `${href}?${q}` : href;
};
