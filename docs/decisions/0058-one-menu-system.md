# 0058: One menu system: every object's menu opened the same way, its entries in the same groups

- Status: accepted (the owner, 2026-10-23: "make it so")
- Date: 2026-10-23
- Builds on: 0021 (menus are Finamp-style sheets: how a menu looks). This decides what is in one, in what order, and
  how it opens. Amends docs/ui-conventions.md's menu, select-mode and long-press lines.

## Context

0021 gave every menu one look: a bottom sheet with the item's card, a row of large actions, entries below. It did not
say which objects have a menu, how one opens, or what goes in it. The owner (2026-10-23): "We have no system for
context menus at all. There is no logic to what is placed in a context menu." An analysis of the app found:

- Opening differs. A long-press opens a track's menu but starts selecting on an album or a playlist. An artist's menu
  is only in its own view. A crate has no menu at all: its actions are header buttons, and a tap on its card's picture
  shuffles it.
- The large actions differ. Start radio is a large action for a track and a playlist, an entry for an artist, and
  missing for an album.
- The entries differ in name ("View album", "View"; "Details", "Info"), place (Delete mid-menu in the album's), and
  grouping (cards for an album and an artist, one flat list for a track and a playlist).
- Actions are missing. Tracks can only be exported by adding them to a playlist. "Exclude globally" is only behind a
  heart's long-press. A crate cannot be exported, shared, or saved as a playlist from a menu.
- Secondary menus (a heart's long-press, the select sheet, the visuals' cable menu) each have their own order.

## Decision

The music app's menus are the reference. The visuals' menus follow them; nothing in the music app changes to match
the visuals (the owner: "streamlining the visualization menu according to the music application menu is good, but we
should not do it the other way around").

### Opening

- **In the music lists** (tracks, albums, artists, playlists, crates, genres, queues): a long-press, a right-click or
  ⋮ opens the object's menu. Selecting starts from the Select button in the list's header; in select mode a tap ticks
  and a long-press asks what to tick (All, None, Others).
- **A badge, chip or link that opens an object** (an album's artist chip, a genre chip, the player's "plays from"
  chip, a track's album or artist) opens that object's menu on a long-press or a right-click (the owner, 2026-10-23:
  "Whenever a badge single click leads to opening an object, then a long press on that badge should lead to opening
  its context menu").
- **In the visuals' rack** a long-press selects (the owner's choice, kept as the rack's own exception). Its menus (a
  cable's, the selection bar's actions, a snapshot's, a port's help) take the music app's shape below.

### One shape

The object's card on top (0021), the large actions, then cards of entries in this order. An object shows only what
applies to it; a card with nothing in it is not shown.

| # | Group | Entries, in order |
|---|---|---|
| A | Large actions | Play (Play now for a track) · Resume · Shuffle · Play next · Add to queue · Start radio |
| B | Go to | View (the object itself, when not already viewed) · Go to album · Go to artist · Go to genre |
| C | Feel | Favourite · Rating · Exclude here · Exclude everywhere |
| D | Collect | Add to playlist · Pin to top · Save as playlist |
| E | Take away | Download · Keep on this device · Export… · Share |
| F | Know | Info · Web search · Genius search |
| G | Change | Rename · Edit · Spoken as… · Picture or cover · Repair… · Sync… · the library's own (Move, Replace, Cancel conversion) |
| H | Remove | Remove from (queue, playlist, crate, this device) · Delete — red, always last |

Play next and Add to queue are a track's, an album's and an artist's, not a playlist's, a crate's or a genre's (the
owner, 2026-10-23: for those "this doesn't really make sense"). A queue's Shuffle is once (a queue always has an order;
Undo puts the order before back): never an on/off state.

Names are the same everywhere: Info (not Details), View for the object itself, Go to for another.

- **A slice** is a menu opened from a part of an object: a heart's long-press shows Feel; a port's long-press shows
  Info. Same card (the part as a chip), only its groups. Whatever a slice holds is also in the full menu.
- **A selection** ("12 tracks") is an object: its menu is the same, applied to all of them.
- **Control sheets** (View, Filter, Sleep, Output, More music, Queues; the visuals' Reset and Graph view) are not
  object menus: a heading, the choices with the current one marked, Reset or Clear last.

### Every object

| | Track | Album | Artist | Playlist | Crate | Genre | Queue |
|---|---|---|---|---|---|---|---|
| A | Play now, Play next, Add to queue, Start radio | Play, Resume, Shuffle, Play next, Add to queue, Start radio | the album's | Play, Shuffle, Start radio | Play, Shuffle, Start radio | Play, Shuffle, Start radio | Shuffle (once) |
| B | Go to album, artist, genre | View, Go to artist, genre | View, Go to genre | View | View | View | – |
| C | Favourite, Exclude here, everywhere | Favourite, Rating, Exclude | Favourite | Favourite | – | – | – |
| D | Add to playlist | Add to playlist | Add to playlist | Pin to top | Save as playlist | Start a crate from it | Save as playlist |
| E | Download, Export…, Share | Download, Keep, Export…, Share | Download, Keep, Export…, Share | Download, Keep, Export…, Share | Download, Keep, Export…, Share | Export… | Export… |
| F | Info, Web, Genius search | Info, Web, Genius search | Info, Web search | Info | Info | – | – |
| G | – | Cover, the library's | Picture | Rename, Spoken as, Repair, Sync | Edit, Picture | – | Rename |
| H | Remove from queue / this playlist / this crate | Remove from this device, Delete album | Remove from this device | Delete playlist | Delete crate (one's own) | – | Delete |

A crate's page header keeps only what shows the crate (its favourites filters) and its ⋮; Keep, Edit and Make playlist
move into its menu. Its card's picture still shuffles it (the owner, 2026-10-19: "a play button on the crates"), as a
cover plays an album; its menu has Shuffle too.

### Export, one step for every object

Export… opens one step (`lib/ExportStep.svelte`), the same from any object or selection:

- **What:** the object's tracks, with a line saying how many, about how large, and how many are missing (left out).
  For a list, with its playlist file (M3U) or not; for several playlists, as they are or combined into one.
- **Format:** Original files · FLAC at CD quality (hi-res converted to 16-bit/44.1 kHz, decision 0013; others as they
  are) · Opus at 96, 128, 160 or 256 kbps (lossless files become Opus, decision 0033's copies; lossy files stay as they
  are: a second lossy pass only loses more).
- **Layout:** one folder, or Artist/Album folders.
- **Covers** inside every file, always (0053).

One zip, streamed as it is made (0007's export, which now takes any tracks and a format).

## Consequences

- Albums and playlists: a long-press opens the menu (before: it started selecting); Select is the header's button.
  Tracks in a list: the same.
- Every menu file orders its entries by the table; reviews check new entries against it.
- The crate, genre and queue menus are new components on MenuSheet; the heart sheet becomes the Feel slice.
- The export endpoint takes tracks (an album, a crate, a selection, a queue) and a format besides playlists.
