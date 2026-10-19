# 0023: One panel: the player and playlists

- Status: accepted
- Date: 2026-10-14
- Builds on: 0018 (the player and its chrome), 0019 (the remote's features), 0021 (menus are sheets), 0022 (the local
  player)
- Superseded in part by: 0025 (a selection of playlists in tabs; the address)

## Context

Cratarr shows no playlist's contents anywhere: the Playlists grid only says whether the playing track is in a
playlist. The owner wants a playlist view, opened from a playlist (its menu), in the panel the player lives in.
Insights already has a panel of its own for the selected playlist (its relations) and for the graph's legend.

Two first ideas did not hold: a playlist in the panel on wide screens but as a page on phones (one feature in two
places with different rules), and no full-screen playlist at all (phones only have the full-screen panel). What
they were after holds: the player never turns into something else by itself, and the player is always one tap away.

## Decision

The owner decided (2026-10-14):

- **One panel, two contents.** The panel is what the player is today: beside the page on a roomy window, the
  full-screen layer on a phone (and on a wide window after Maximise). It shows the player, or a playlist that was
  opened. A playlist shows only because it was opened; nothing inside the player switches to it.
- **A short stack.** An opened playlist lies under the player: tapping the mini-player brings the player over it;
  folding or closing the player (its chevron, a swipe down) shows the playlist again. ✕ in the playlist view (and Esc,
  and Back) closes the playlist: the player shows if something plays, else the panel hides (wide) or the layer
  closes (phone).
- **The mini-player stands in for the player** whenever the panel does not show it. Layers, bottom to top: the page,
  the playlist (full height, ending above the mini-player), the mini-player, the player (covers everything), sheets.
  On a phone an opened playlist is therefore a full-height layer with the mini-player at its foot, as Finamp's
  album screens with its now-playing bar.
- **The default and empty states:** nothing playing and nothing opened, no panel (as today); something playing, the
  player (as today); a playlist opened with nothing playing, the playlist without a mini-player, closed with ✕.
- **Shell-level, the same on every size:**
  - The opened playlist is in the address, as a query on whichever page is shown (`#/insights?open=Chill`): Back
    closes it, a reload brings it back.
  - It stays open when the window crosses the breakpoint (turning the tablet): on a phone size the layer opens with it.
  - One panel width, whatever it shows.
  - It stays open while changing page.
- **Opening:** "Open" in the playlist menu (everywhere the menu is: the Playlists grid, Insights). Since the chips got
  Finamp's check/plus circle (owner, 2026-10-14), a chip is two buttons: its label opens the playlist, the circle adds or
  removes the playing track (this replaces the double-click first planned). Opening another playlist replaces
  the open one (no stack of playlists); opening the open one scrolls it to the top.
- **The playlist view:**
  - Its header: the mosaic, the name, "N tracks · length", the sync chips, the playlist menu, Play and Shuffle (on the
    chosen output), and "Add what's playing" when the playing track is not in it.
  - Two tabs, a segmented control: **Tracks** and **Related**. Opened from Insights it starts on Related, elsewhere
    on Tracks. **Deferred** (owner, 2026-10-14): until the tabs come, Related is a compact section between the header
    and the tracks (focus and the closest playlists; the genres sit in the header, as Finamp's genre line).
  - Styled as Finamp's playlist screen in less room: a bar with ✕ and the menu (no title); the covers (125 px) beside
    the name, "♪ N tracks (length)" and the genres; Play, Shuffle, Play next and Add to queue as icons over labels;
    rows as the queue's with the album after the artist; the playing track tinted in the cover's colour with a small
    visualiser on its cover. A swipe left on a row plays it next (Finamp's default swipe there) and springs back.
  - Tracks lists the playlist in file order (the `.m3u` is the truth). Its rows are as the queue's (decision 0019);
    the playing track is marked. A tap plays the playlist from that track on the chosen output and the view stays
    (the mini-player shows it); a long-press is that track's menu.
  - Entries missing on disk, or not yet matched to the server, show by their file name, greyed: not hidden.
  - The states:
    - Loading: the header shows at once (the grid knows it), the rows when they arrive.
    - Empty: "No tracks yet.", and "Add what's playing" when something plays.
    - Failed: one short error line.
  - Events: a rename is followed, a delete closes the view, changes (sync, a chip tap) show in place. Stopping
    playback or switching output leaves it as it is.
  - The rows come in one batch request with their details; only rows on screen are drawn.
- **Playlist favourites** (owner, 2026-10-14, as Finamp's): the heart in the view's bar, "Add to / Remove from
  favourites" in the playlist menu, and a small red heart on a favourite's chip above its problems' dot (no wider
  chip). A Cratarr playlist can be in Jellyfin more than once (imported from the `.m3u`, an account's own synced copy):
  the heart marks every copy, for the account Cratarr plays as, and shows when any copy is a favourite.
- **Insights consolidates into the panel.** Its playlist panel (tracks and genres, focus, closest playlists and what
  they share, its menu) becomes the view's Related tab, available wherever a playlist is opened. On Insights the
  graph's selection and the opened playlist are one state while the panel shows a playlist: opening one highlights
  it in the graph, tapping another point or a "closest" link replaces it. A tap on a point only selects it; the
  graph's button opens it. The legend is not about a playlist: it becomes a sheet from the Insights toolbar. The
  insights data and the closest-playlists ranking move out of the Insights page into one shared store (with
  `lib/proximity.ts`), read by the page and the Related tab, fetched once. `#/insights?playlist=` gives way to
  `?open=` (old links are translated).
- **Stages:**
  1. A track menu for any track, not only the playing one: needed by the playlist view's and the queue's rows alike.
  2. The read-only playlist view, the panel's stack and layers, and the Insights consolidation.
  3. Editing: remove (swipe) and reorder (handle), written to the `.m3u` through the store (backed up, with undo). A
     linked playlist's edits travel with its sync (decision 0014). As built (2026-10-14), as Finamp's playlist edit
     screen: an edit mode entered with the pencil in the view's bar (a swipe left outside it stays Play next); rows get
     the reorder handle and ✕, a swipe left removes too; removed tracks wait under "Removed tracks" to be restored to
     their place; Save writes the playlist once (only reordering or leaving out its own lines, refused when it changed
     since), Cancel drops the edit. "Names" (beside Save) shows the playlist's name and the names it is spoken as
     (decision 0015) in place of the info beside the cover; Save renames it last, and the panel, pin and heart follow.

## Consequences

- The shell owns the panel's content (player or playlist) and its address; pages no longer have panels of their own.
- Insights loses its own panel, legend panel, their state, keys and address handling.
- The server gains a batch of a playlist's rows with their details (and their file names for unmatched entries).
- `docs/ui-conventions.md` describes the panel this way when stage 2 ships.

## Amendment (2026-10-19): swipe right plays next, swipe left removes

The owner: "on any library content list in the panel, swiping to the left should remove it from that list. So this goes
for crates and for playlists. Also be sure to use the standard way of revealing the action during the swipe on the
row." Of where Play next goes: swipe right, everywhere.

- **Left removes** a track from the list it is in: one's own playlist (its line out of the `.m3u` at once, with Undo; in
  the several-playlists view, out of its section's playlist) or a crate (its left-out list, decision 0024 amended).
  An album's and an artist's tracks are no list of one's own: a swipe left does nothing there. Another user's shared
  playlist is not one's own: nothing either. Edit mode keeps its swipe left (removed until Save).
- **Right plays next**, on every list of tracks in the panel (it was a swipe left, Finamp's default, until now).
- **What the swipe does shows as the row moves**: in the strip it uncovers, growing from the edge, the action's icon
  and word (red Remove, the accent's Play next) on a light tint (`lib/SwipeBehind.svelte`). The queue's swipe left
  shows its Remove the same way.
- A row's menu says the same: "Remove from <list>" after Remove from queue's place.
