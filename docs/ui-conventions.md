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
    title's second line and the progress bar give way, in that order. Playing on this device the progress bar is a
    slider (Finamp's): drag the handle and the time follows, the player seeks on release; arrow keys move 5 s. Its background is the cover, blurred under a veil; its accent comes from the cover. On the
    cover: tap plays or pauses, double-tap is the favourite, swipe left or right skips; anywhere: swipe up shows the
    playlists, swipe down closes.
  - **Where it shows:** with room for it (a content area of 800 × 500 px or more) the full player sits beside the page
    by itself while something plays, resizable at its edge (handle in the middle), the width remembered; its chevron
    folds it to the mini-player until that is tapped. Otherwise the mini-player shows, and opens the full player
    **over everything**, navigation included (slides up, fades out); its chevron, Esc or a swipe down close it, and
    so does changing page. Maximise (beside the page) and back-beside-the-page (over it) are remembered.
  - **Queue** (`lib/QueueSheet.svelte`, as Finamp's queue sheet): a sheet over the player on the blurred cover, a grab
    handle and "Queue" (drag it down to close); the previous tracks folded under "Previous Tracks" (this device only);
    the playing track as a card pinned at the top that fills in the cover's colour as it plays, with play/pause on its
    cover, the heart and the track menu; "Playing from …" (or "Playing on …" for the server's player) and "i / n
    (time left)" pinned under it; then what comes next, 70 px rows (60 px cover, title up to two lines, artist,
    length). A tap plays a row. Playing on this device a row's handle reorders and a swipe left past 65 % removes it,
    with no undo, as Finamp; the server's player takes neither command.
  - **Output** (`lib/OutputSheet.svelte`, decisions 0019 and 0022): the speaker button in the actions row (menu,
    heart, count, output, queue) opens a "Play on" sheet: This device, and the server's player by its name, the
    current one checked. Choosing the other one moves the music there (the queue and the position) and stops the side
    that played; nothing asks first, the player simply carries on there. The choice is remembered by the browser.
    Whichever side plays, the player looks and works the same; playlists, the heart and the info chips act on the
    track that plays there. A track this device cannot play is skipped with one error line naming it.
  Pages never show their own now-playing; they read the shell's shared state (`lib/live.svelte.ts`,
  `lib/remote.svelte.ts`).
- **Widths:** grids (the playlist chips) use the full width; lists and forms stop at 900 px, left-aligned.
- **An opened playlist** (decision 0023, `lib/PlaylistView.svelte`): in the panel beside the page, or on a phone a
  full-height layer with the mini-player floating over its end. Its bar has ✕, the heart (Finamp's playlist
  favourite), the pencil (edit mode) and the menu; in
  edit mode Cancel, Names and Save. Rows as the queue's; edit mode as Finamp's playlist edit screen: Names swaps the
  info beside the cover for Name and Spoken as (where Finamp has its name field); rows with handle and ✕, swipe to
  remove, "Removed tracks" to restore; all written once on Save, a rename last (the panel follows the new name).
- **Crates** (decision 0024, between Incoming and Playlists): the permanent crates (Favourites, Unlisted, Unlisted
  favourites) as chips with their live counts, styled as the playlist chips (the open one bordered). A crate opens in
  the panel as a playlist does, without the statistics and editing; its rows come in pages, newest first; the save icon
  (where a playlist has the pencil) makes a playlist of it, which then opens. "New crate" (and the pencil on one's own
  crate) opens the **crate builder** in the panel: a name, then conditions as cards (the axis, an is/NOT toggle, its
  values as chips, a range as two numbers, a yes/no axis as just its name; negated cards tinted red), "Add condition"
  offering the server's axes, and below a live preview (the count and the first tracks). Cancel and Save in the bar;
  Delete at the end for one's own crate (a second tap confirms). The draft is kept in the browser until Save or
  Cancel.
- **Playlist chips** (the Playlists page) look like Finamp's add-to-playlist entries: tinted in the accent, 10 % when
  the playing track is not in the playlist and 30 % when it is; the name (two lines at most), its marks, the count, a
  divider and Finamp's plus circle or filled check circle. A tap on the circle adds or removes the playing track; a
  tap on the label opens the playlist (decision 0023). A favourite playlist has a small red heart (Finamp's list mark)
  above the problems' dot, in the dot's place, so the chip gets no wider. The playlist open in the panel has a border
  in the accent and no extra fill: fill means "holds the playing track", the border "is shown in the panel". When the playing
  track is outside Music (it cannot go in playlists), the chips it is not in are quieter and their plus circle is
  dashed (Finamp's mark for a playlist it cannot add to); a tap says why.
- **Pinned playlists** (the playlist menu: Pin to top / Unpin) come first on the Playlists page, in the order they
  were pinned, marked with a small pin after the name; "✓ first" applies within the pinned and within the rest. Pins
  are remembered by the browser, like the sort order and size; a rename keeps the pin.
- **Controls sit next to what they change.** Sort order and size of the playlist grid are in the Playlists toolbar,
  not in Settings. Where music plays is chosen at the player (its output button), not in Settings.
- **Views of one page are tabs**: a segmented control at the start of its toolbar, remembered by the browser
  (Insights: Statistics, Related). Its switches (Related: shared tracks, genre, artists) sit in the same toolbar.
- **Insights → Related and the panel** (decision 0023): selecting a point shows "Open <playlist>" at the graph's top
  right, which opens it in the shell's panel (`?open=`); while a playlist is open the graph's selection and the panel
  follow each other. The **Legend** button is always in the graph's bottom-right corner; it opens the legend as a sheet
  (lines, points, and the colour groups named by their most common genres).
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
