// The panel's content (decision 0023): the player, or a playlist that was opened, lying under the player. The opened
// playlist is in the address as `?open=` on whichever page shows (Back closes it, a reload brings it back), and it
// stays open across pages: the navigation keeps it in its links (`keepOpen`).

export const panel = $state({
  /** The opened playlist (its id), or null. */
  open: null as string | null,
  /** The player shows over the opened playlist (the mini-player was tapped); folding the player shows it again. */
  playerTop: false,
  /** Bumped when the open playlist is opened again: its view scrolls back to the top. */
  again: 0,
});

const query = () => new URLSearchParams(location.hash.split('?')[1] ?? '');
const base = () => location.hash.split('?')[0] || '#/';
const address = (name: string) => `${base()}?open=${encodeURIComponent(name)}`;

/** Opened in this visit by pushing an address: closing goes Back, so the history stays tidy. */
let pushed = false;

// Old Insights links (`#/insights?playlist=…`) open the playlist the new way.
{
  const q = query();
  const old = q.get('playlist');
  if (old && !q.get('open')) history.replaceState(null, '', address(old));
}
panel.open = query().get('open');

addEventListener('hashchange', () => {
  const open = query().get('open');
  if (open !== panel.open) {
    panel.open = open;
    panel.playerTop = false;
  }
  if (!open) pushed = false;
});

/** Opens a playlist in the panel. One that is already open is replaced, without a step in the history (no stack of
 *  playlists: ✕ and Back then close it rather than going to the one before). */
export function openPlaylist(name: string) {
  panel.playerTop = false;
  if (panel.open === name) {
    panel.again++;
    return;
  }
  if (panel.open) {
    history.replaceState(null, '', address(name));
    panel.open = name;
    return;
  }
  pushed = true;
  location.hash = address(name);
}

/** Closes the opened playlist (✕, Esc): the player shows if something plays, else the panel goes. */
export function closePlaylist() {
  panel.playerTop = false;
  if (pushed) {
    pushed = false;
    history.back();
  } else {
    history.replaceState(null, '', base());
    panel.open = null;
  }
}

/** A renamed playlist stays open under its new name; a deleted one closes. */
export function playlistRenamed(from: string, to: string) {
  if (panel.open !== from) return;
  history.replaceState(null, '', address(to));
  panel.open = to;
}
export function playlistDeleted(id: string) {
  if (panel.open === id) closePlaylist();
}

/** A page's link that keeps the opened playlist open. */
export const keepOpen = (href: string) => (panel.open ? `${href}?open=${encodeURIComponent(panel.open)}` : href);
