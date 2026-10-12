# Interface conventions

How Cratarr's pages are built and worded. Every change to the interface follows this; a change that needs an
exception changes this file first. Agreed with the owner on 2026-10-12.

## Structure

- **Pages:** Incoming (only when an incoming library is set), Playlists, Repairs, Settings. One thing lives in one
  place: no setting, list or action appears on two pages.
- **No page titles.** The navigation shows where you are. A page starts with the now-playing header if it has one
  (Playlists, Incoming), then its toolbar, then its content.
- **Widths:** grids (the playlist chips) use the full width; lists and forms stop at 900 px, left-aligned.
- **Controls sit next to what they change.** Sort order and size of the playlist grid are in the Playlists toolbar,
  not in Settings. The player choice is in the now-playing header ("Playing on Finamp · Lenovo Tab ▾"), not in
  the sidebar.
- **Settings** has four sections, in this order: Connections (servers: address, key, status), Libraries (Music and
  Incoming per server, extra folders, catalogue status), Display (theme, progress bar), About (version, port, file
  locations). Technical details go to About or nowhere.

## Gestures

- **Tap does the page's main action** on an item: on Playlists add or remove the playing track; on Incoming play
  the album.
- **Long-press (right-click with a mouse) opens the item's menu**, the same kind of menu on every page, with every
  other action for that item (for a playlist: Play, Shuffle, Play next, Add to queue, Rename, Delete).
- A long-press never also triggers the tap, and suppresses the browser's own menu and text selection.

## Words

- Plain words, sentence case ("Add to queue", not "Add To Queue"). No internal terms on screen: not "source",
  "placement", "root", "entry", "line", "sync".
- **Vocabulary:**
  - *track*, *album*, *playlist*.
  - *Music* and *Incoming*: the two library roles, named exactly as in Settings.
  - *missing track*: a playlist line whose file is gone.
  - *player*: an app on a device, written "Finamp on Lenovo Tab".
  - *server*: Jellyfin and the like (not "source").
- **Counts say what they count:** "in 3 playlists", "12 tracks", "92 missing tracks".
- **Buttons are verbs** saying what happens: "Fix 92 missing tracks", "Delete playlist", not "OK" or "Submit".
- **Help text** is one sentence under the thing it explains, only where the label is not enough.

## Feedback

- **One notice style everywhere:** a line under the toolbar. Neutral for done ("Renamed to Chill."), amber for
  warnings, red for errors, and an "Undo" link when the action can be undone. It disappears after a few seconds
  unless it is an error.
- **Destructive actions** ask in place, naming what goes ("Delete Chill?"); nothing destructive happens on one tap.
- **Loading:** "Loading…" in the content area; never an empty page. **Empty:** say why and what to do ("Nothing
  incoming." / "No incoming library is set. Choose one in Settings → Libraries.").

## Checking

- Every interface change is looked at on a phone width (390 px) and a desktop width before release, with real
  data, and checked against this file.
