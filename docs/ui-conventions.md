# Interface conventions

How Cratarr's pages are built and worded. Every change to the interface follows this; a change that needs an
exception changes this file first. Agreed with the owner on 2026-10-12.

## Structure

- **Pages:** Incoming (only when an Incoming library is set), Playlists, Insights, Repairs, Settings. One thing lives in one
  place: no setting, list or action appears on two pages.
- **No page titles.** The navigation shows where you are. A page starts with its toolbar, then its content.
- **The now-playing header is part of the app shell**, sticky above the pages where what plays is acted on: Playlists
  and Incoming. Repairs, Insights and Settings have no player. It shows the cover (library cover, else the
  album image from the server), title, artist · album, progress, "in N playlists", and the transport controls
  (previous, play/pause, next), a favourite heart (the player's account's Jellyfin favourite) and an expand button.
  The active player is always named in a small pill, even when there is only one. Below 480 px the count, heart and
  expand sit under the cover, so the track's column has the width, and the progress bar takes its own line.
- **Full-screen player:** opened from the cover, the title or the expand button; it covers the page but never the
  navigation, which stays usable and closes it when used. Also closed by its collapse button, Esc, or a swipe down.
  It shows the same things, larger, in one centred column, and always fits without scrolling.
  Pages never show their own now-playing; they read the shell's shared state (`lib/live.svelte.ts`).
- **Widths:** grids (the playlist chips) use the full width; lists and forms stop at 900 px, left-aligned.
- **Controls sit next to what they change.** Sort order and size of the playlist grid are in the Playlists toolbar,
  not in Settings. The player choice is in the now-playing header ("Playing on Finamp · Lenovo Tab ▾"), not in
  the sidebar.
- **Views of one page are tabs**: a segmented control at the start of its toolbar, remembered by the browser
  (Insights: Statistics, Related). Its switches (Related: shared tracks, genre, artists) sit in the same toolbar.
- **The playlist panel (Insights → Related only):** selecting a point shows a button at the graph's top right that
  opens the panel. The graph never gets narrower: the panel sits beside it where there is room, else over its right
  side (full width on a phone). While open it follows the selection; ✕, Esc, Back or a tap on empty space closes it.
  It has its own address (`#/insights?playlist=…`). With nothing selected, the same button opens the legend (lines,
  points, and the colour groups named by their most common genres).
- **Settings** has four sections, in this order: Connections (servers: address, key, status; Lidarr the same way, shown
  only when an Incoming library is set), Libraries (one row per
  server library: its use, Music, Incoming or Not used, and the folder Cratarr sees its files in, or a warning when
  it can't; then the catalogue status and tag splitting), Display (theme, progress bar), About (version, port, file
  locations). Technical details go to About or nowhere. A use is set only there (decision 0009).

## Gestures

- **Tap does the page's main action** on an item: on Playlists add or remove the playing track; on Incoming play
  the album; on Insights' graph highlight the playlist and its closest playlists.
- **Long-press (right-click with a mouse) opens the item's menu** when the item has more than one other action: a
  small popover anchored to the item, styled like the rest of the app, pointing at it, flipping to stay on screen.
  The same component on every page. For a playlist: Play, Shuffle, Play next, Add to queue, Rename, Delete.
  Esc, a tap outside or scrolling closes it. With a single other action, the long-press does that action directly.
- Inside the popover: actions as a short list, destructive ones last and in red. Rename and the delete confirmation
  happen inside the same popover, not in a separate dialog.
- A long-press never also triggers the tap, and suppresses the browser's own menu and text selection.

## Words

- Plain words, sentence case ("Add to queue", not "Add To Queue"). No internal terms on screen: not "source",
  "placement", "root", "entry", "line", "sync".
- **Vocabulary:**
  - *track*, *album*, *playlist*.
  - *Music* and *Incoming*: the two library uses, named exactly as in Settings.
  - *missing track*: a playlist line whose file is gone.
  - *player*: an app on a device, written "Finamp on Lenovo Tab".
  - *server*: Jellyfin and the like (not "source").
- **Counts say what they count:** "in 3 playlists", "12 tracks", "92 missing tracks".
- **Buttons are verbs** saying what happens: "Fix 92 missing tracks", "Delete playlist", not "OK" or "Submit".
- **Help text** is one sentence under the thing it explains, only where the label is not enough.

## Feedback

- **What an action did, or why it failed, is a toast** (`lib/toast.svelte.ts`): one at a time, floating above the
  bottom bar (beside the sidebar from 1024 px), never moving the page. Neutral for done ("Deleted Chill."), amber for
  warnings, red for errors, with "Undo" when the action can be undone. Done and warnings go after a few seconds;
  errors stay until dismissed, replaced, or the page changes. Errors say what failed in plain words, never a bare
  status code.
- **A page's own state stays in place** (`lib/Notice.svelte`): the page could not load *and has nothing to show*, a
  field's question (the Keep year), empty and loading states. Nothing temporary sits in the page: a reload or update
  that fails while the page shows its content is a toast (once, gone when the server answers again), and so is the
  answer to a check (Check connection: connected or why not).
- **Waiting:** a thin bar along the top while something the user asked for loads (`lib/ProgressBar.svelte`): only
  after 300 ms, so quick answers show nothing. Background polling and actions shown at once (the player, playlist
  taps, the favourite) never show it (`quietly` in `api.ts`). The button that was tapped shows a spinner in place of
  its label (`aria-busy` with `disabled`) until the answer. No full-page spinners.
- **Destructive actions** ask in place, naming what goes ("Delete Chill?"); nothing destructive happens on one tap.
- **Loading:** "Loading…" in the content area; never an empty page. **Empty:** say why and what to do ("Nothing
  incoming." / "No Music library is set. Choose one in Settings → Libraries.").

## Checking

- Every interface change is looked at on a phone width (390 px) and a desktop width before release, with real
  data, and checked against this file.
