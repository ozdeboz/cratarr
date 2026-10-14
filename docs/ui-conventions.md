# Interface conventions

How Cratarr's pages are built and worded. Every change to the interface follows this; a change that needs an
exception changes this file first. Agreed with the owner on 2026-10-12.

## Structure

- **Pages:** Incoming (only when an Incoming library is set), Playlists, Insights, Repairs, Settings. One thing lives in one
  place: no setting, list or action appears on two pages.
- **No page titles.** The navigation shows where you are. A page starts with its toolbar, then its content.
- **Navigation:** from 1024 px a sidebar on the left; below it a bar of pages fixed along the **top** (icon and label,
  up to five, the rest under More).
- **The player is part of the app shell** (decision 0018, after Finamp), on every page: since Cratarr plays music
  itself (decision 0022) it is a player, not only a remote for adding what plays. The server picker stays on the pages
  that act on what a server plays: Playlists and Incoming.
  - **Mini-player** (`lib/MiniPlayer.svelte`): a bar floating along the bottom, 64 px high, 12 px from the edges,
    corners 12 px: the cover with play/pause on it, the title (scrolling when it does not fit), artist and time, and
    the heart (the player's account's Jellyfin favourite). The bar fills from the left as the track plays, in the
    cover's colour. Tap or swipe up opens the full player; swipe left or right skips; swipe down does nothing.
  - **Full player** (`lib/NowPlaying.svelte`): the cover (library cover, else the album image from the server), the
    active player named in a small pill (even when there is only one), title, artist · album, the transport controls
    (previous, play/pause as a rounded square, next), progress, the heart and "in N playlists". It lays itself out
    for its space (`lib/playerLayout.ts`, a port of Finamp's layout): portrait or landscape, whichever gives the
    larger cover; spare room spreads its three groups (track name; chips, progress and buttons; the actions row);
    when short of height the large gaps, the large play button, the pill, the small gaps, the info chips, the
    title's second line and the progress bar give way, in that order. Its background is the cover, blurred under a veil; its accent comes from the cover. On the
    cover: tap plays or pauses, double-tap is the favourite, swipe left or right skips; anywhere: swipe up shows the
    playlists, swipe down closes.
  - **Where it shows:** with room for it (a content area of 800 × 500 px or more) the full player sits beside the page
    by itself while something plays, resizable at its edge (handle in the middle), the width remembered; its chevron
    folds it to the mini-player until that is tapped. Otherwise the mini-player shows, and opens the full player
    **over everything**, navigation included (slides up, fades out); its chevron, Esc or a swipe down close it, and
    so does changing page. Maximise (beside the page) and back-beside-the-page (over it) are remembered.
  - **Output** (`lib/OutputSheet.svelte`, decisions 0019 and 0022): the speaker button in the actions row (menu,
    heart, count, output, queue) opens a "Play on" sheet: This device, and the server's player by its name, the
    current one checked. Choosing the other one moves the music there (the queue and the position) and stops the side
    that played; nothing asks first, the player simply carries on there. The choice is remembered by the browser.
    Whichever side plays, the player looks and works the same; playlists, the heart and the info chips act on the
    track that plays there. A track this device cannot play is skipped with one error line naming it.
  Pages never show their own now-playing; they read the shell's shared state (`lib/live.svelte.ts`,
  `lib/remote.svelte.ts`).
- **Widths:** grids (the playlist chips) use the full width; lists and forms stop at 900 px, left-aligned.
- **Controls sit next to what they change.** Sort order and size of the playlist grid are in the Playlists toolbar,
  not in Settings. Where music plays is chosen at the player (its output button), not in Settings.
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
- **Long-press (right-click with a mouse) opens the item's menu** when the item has more than one other action. Every
  menu is a **bottom sheet like Finamp's track menu** (decision 0021, `lib/MenuSheet.svelte` + `lib/MenuEntry.svelte`),
  the same component on every page: at most 500 px wide, rising from the bottom, a drag handle; the item on a card at
  its top (its image: a cover, or a playlist's mosaic of its first four albums; its title; small chips; for a track
  the heart); its playback actions as a row of large icons with small labels; then the other entries as rows with an
  icon in the accent colour. The sheet takes its look from the item's image (blurred behind it, its accent).
  For a playlist: Play, Shuffle, Play next, Add to queue; Sync to…, Spoken as…, Rename, Delete. For the playing track
  (the player's menu button, or a right-click on its cover): Play next, Add to queue; Add to playlist, the favourite,
  New playlist from the queue. A tap on the backdrop, Esc, Back or dragging the sheet down closes it. With a single
  other action, the long-press does that action directly.
- Inside the sheet: destructive entries last and in red. Sync to…, Spoken as…, Rename, the playlists for a track and
  the delete confirmation are steps inside the same sheet (it grows or shrinks to fit, a back arrow returns), not a
  separate dialog.
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
  mini-player, or the bottom edge without it (beside the sidebar from 1024 px), never moving the page. Neutral for done ("Deleted Chill."), amber for
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
