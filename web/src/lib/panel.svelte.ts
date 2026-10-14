// The panel's subject (decisions 0023, 0025): the player, or what was opened lying under it: a selection of one or more
// playlists in tabs (Tracks, Info, Export, Repair, Sync), or a crate (`crate:…`, its builder `crate-new`, `crate-edit:…`).
// It is in the address as `?open=…` (repeated for each playlist) and `&tab=…` on whichever page shows (Back closes
// it, a reload brings it back), and it stays open across pages: the navigation keeps it in its links (`keepOpen`).

export type Tab = 'tracks' | 'info' | 'export' | 'repair' | 'sync';
export const TABS: { tab: Tab; label: string }[] = [
  { tab: 'tracks', label: 'Tracks' },
  { tab: 'info', label: 'Info' },
  { tab: 'export', label: 'Export' },
  { tab: 'repair', label: 'Repair' },
  { tab: 'sync', label: 'Sync' },
];
const isTab = (t: string | null): t is Tab => TABS.some((x) => x.tab === t);

/** A crate, or its builder: a subject of one, without tabs (decision 0025). */
export const isCrateSubject = (open: string | null) => !!open && (open.startsWith('crate:') || open === 'crate-new' || open.startsWith('crate-edit:'));

export const panel = $state({
  /** What was opened: the (first) playlist's id, or a crate's key; null when the panel shows the player only. */
  open: null as string | null,
  /** The playlists selected, in the order ticked; empty for a crate. */
  selection: [] as string[],
  tab: 'tracks' as Tab,
  /** The player shows over what was opened (the mini-player was tapped); folding the player shows it again. */
  playerTop: false,
  /** Bumped when the open playlist is opened again: its view scrolls back to the top. */
  again: 0,
});

const query = () => new URLSearchParams(location.hash.split('?')[1] ?? '');
const base = () => location.hash.split('?')[0] || '#/';
function address(names: string[], tab: Tab) {
  const q = new URLSearchParams();
  for (const n of names) q.append('open', n);
  if (tab !== 'tracks') q.set('tab', tab);
  return `${base()}?${q}`;
}

// The history entries the panel added in this visit carry how many there are: closing goes Back over all of them, so
// the history stays tidy (opening adds one; a tab switch in the full-screen panel adds one more).
const depth = (): number => (history.state && typeof history.state.panel === 'number' ? history.state.panel : 0);
function push(url: string) {
  history.pushState({ panel: depth() + 1 }, '', url);
}
function replace(url: string) {
  history.replaceState(history.state, '', url);
}

function read() {
  const q = query();
  const open = q.getAll('open');
  const first = open[0] ?? null;
  if (first !== panel.open || open.join('\n') !== panel.selection.join('\n')) panel.playerTop = false;
  panel.open = first;
  panel.selection = first && !isCrateSubject(first) ? open : [];
  const tab = q.get('tab');
  panel.tab = panel.selection.length && isTab(tab) ? tab : 'tracks';
}

// Old Insights links (`#/insights?playlist=…`) open the playlist the new way.
{
  const q = query();
  const old = q.get('playlist');
  if (old && !q.get('open')) replace(address([old], 'tracks'));
}
read();
addEventListener('hashchange', read);
addEventListener('popstate', read);

/** Opens playlists (one, or a selection) or a crate in the panel, on `tab`. What is already open is replaced, without
 *  a step in the history (no stack: ✕ and Back then close it rather than going to the one before). */
export function openSelection(names: string[], tab: Tab = 'tracks') {
  if (!names.length) return closePlaylist();
  const t = isCrateSubject(names[0]) ? 'tracks' : tab;
  panel.playerTop = false;
  if (names.join('\n') === (panel.selection.length ? panel.selection : [panel.open]).join('\n') && t === panel.tab) {
    panel.again++;
    return;
  }
  if (panel.open) replace(address(names, t));
  else push(address(names, t));
  read();
}

/** Opens one playlist (or a crate) on its Tracks. */
export const openPlaylist = (name: string) => openSelection([name]);

/** Another tab for the selection: a step in the history only when the panel is full-screen (Back returns to it). */
export function setTab(tab: Tab, fullScreen: boolean) {
  if (tab === panel.tab || !panel.selection.length) return;
  const url = address(panel.selection, tab);
  if (fullScreen) push(url);
  else replace(url);
  read();
}

/** Closes what was opened (✕, Esc): the player shows if something plays, else the panel goes. `then` runs once it
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

/** A renamed playlist stays open under its new name; a deleted one leaves the selection. */
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

/** A page's link that keeps what is open open. */
export const keepOpen = (href: string) => {
  const q = location.hash.split('?')[1];
  return panel.open && q ? `${href}?${q}` : href;
};
