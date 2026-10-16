# 0025: The panel: a subject in tabs

- Status: accepted
- Date: 2026-10-14
- Supersedes in part: 0023 (what the panel shows for playlists, and its address), 0014 (Sync to… as a step in the
  playlist's menu), the Repairs page (ui-conventions)
- Builds on: 0007 (export), 0014 (sync links), 0023 (one panel), 0024 (crates)

## Context

Export (0007) needed a place, and putting it in the panel showed the pattern behind several features: a playlist is
looked at (its tracks), exported, repaired and linked, and some of that is worth doing for several playlists at once.
The player is different: it is always about one track, in one view. The owner settled the model on 2026-10-14.

## Decision

- **The panel shows a subject.** In player mode the subject is the playing track, in one view (as 0018–0023). In
  playlist mode the subject is a **selection of one or more playlists**, shown in **tabs: Tracks, Info, Export,
  Repair, Sync.** Every tab is always there (a tab with nothing to do says so); the panel opens on Tracks. A crate (0024) is a
  subject of one, with Tracks only: crates never join a selection, since export and repair need fixed playlists.
- **Opening a playlist is selecting one.** A tap on a chip's name opens it: the selection becomes that playlist.
- **Select** (the Playlists page's toolbar) lets the selection grow: a tap on a chip's name ticks or unticks it. A
  ticked chip has the border of the playlist open in the panel (one state, one look); **the circle keeps adding or
  removing the playing track**, in Select too. On a roomy window the panel shows the selection as it is ticked; on a
  phone, where the panel covers the page, one ticks first and opens it with "Open 3 playlists". Select stays highlighted while on; a tap on it again ends it; an open panel then keeps the playlist ticked last.
  A ⋮ in the page's top right corner opens the playlist menu for the ticked playlists.
- **One menu, one panel, for one playlist or several.** The playlist menu (a long-press, the panel's ⋮, the corner ⋮)
  takes one or more: Play, Shuffle, Play next, Add to queue (several: one playlist after another, in the order ticked,
  each track once), Open, favourite, Pin, Export…, Sync to…, Delete (one Undo for all); Spoken as… and Rename only
  for one. The panel's bar (✕, heart, ⋮) and its Play row (on every tab, under the header) are the same for several; only one playlist has the pencil
  (its edit mode). 0023's "Add what's playing" goes: the chips' circle adds the playing track.
- **Tracks:** one playlist as 0023 (its edit mode only here); several as **a section per playlist**, in the order
  ticked: a header (name, count) that folds, its tracks below, read-only; a tap plays that playlist from the track.
- **Export:** 0007, for the selection; a playlist can be left out there, and more added (Select).
- **Repair:** for the selection, what the Repairs page offered for playlists (found in another format; likely match;
  repeated tracks), plus **Not found**: missing tracks with no file anywhere, each with Remove, and Remove all. Every
  change can be undone.
- **Sync:** 0014's targets for the selection. A target linked for only some shows a dash and "3 of 5"; ticking
  links all, unticking unlinks all; direction and conflict rule show "Mixed" until one is chosen for all.
- **A playlist's menu:** Export… and Sync to… open the panel on that tab; the menu's Sync step goes.
- **The address** holds the selection and the tab: `?open=Chill&open=Analogic&tab=export` (a crate:
  `?open=crate:…`). A reload brings both back. Switching tabs adds a Back step only while the panel is full-screen
  (Back then returns to the previous tab before closing); beside the page, and for ticking, the address is replaced.
- **✕ closes the panel** (and Esc and Back): what was opened goes and the player does not show in its place; it folds
  to the mini-player, whose tap brings it back beside the page. This amends 0023's stack, where closing showed the
  player that lay under it. Folding the player (its chevron) ends in the same state.
- **The Repairs page goes.** Its playlist parts are the Repair tab. What is about the library's files (files that
  cannot be read, damaged tags, no tags, no length) moves to a new page, **Health**, beside Settings on every screen
  size.

## Consequences

- The tab row is the panel's one way to more about a playlist; new views about playlists become tabs, and one-tap
  actions stay in menus.
- **Info** holds what Insights knows of a playlist (how focused it is, its closest playlists; 0023's Related section,
  which leaves Tracks); several playlists get a section each. The genres stay in the header.
- Old `#/repairs` links open the Playlists page.

## Amendment (2026-10-14): an album is a subject

An album (`?view=album:<server>/<id>`) shows in the panel as a crate does, without tabs: its cover, name, artist and
year, its tracks in play order as a playlist's rows, and all four actions (Play, Shuffle, Play next, Add to queue). It
comes from the server (`GET /api/albums/{server}/{id}`), so any album can show: an Incoming album's menu has View (a tap
still plays it), and a track's menu has View album. An Incoming album that is kept, replaced or deleted closes; one
Lidarr adds to is read again. The local player takes its tracks from the same answer, with their names.

## Amendment (2026-10-14): View, not Open

Showing something in the panel is called **View** everywhere: the menus' entry (View, View album; the square with an
arrow out of its corner), the rows' labels, Select's "View N playlists", Insights' "View <playlist>", and the address
(`?view=…`, in place of `?open=…` above and in 0023 and 0024). Old links (`?open=…`, Insights' `?playlist=…`) are no longer
translated: there were none.

## Amendment (2026-10-15): a playlist never holds a track twice (reversed 2026-10-16, below)

A repeated track in a playlist is always a mistake (the same track added twice), so there is nothing to choose: a
playlist holds each track once, the first copy (its oldest place) stays. Every write keeps only the first copy, the
sync's merge (0012) never makes a repeat, and each sync pass takes the repeats out of a file that came with them
(edited by hand, put back from a backup). The Repair tab loses its Repeated tracks section, and `/api/duplicates` goes.

Found when the playlists became copies in your account (0014, amended 2026-10-15): Jellyfin holds a track in a playlist
it makes only once, and the sync took each lost repeat for a removal there. That did what this rule does, the first
copy kept, in six playlists.

## Amendment (2026-10-16): repeats are the user's to choose

The rule above is reversed. Found with Symfonium (an OpenSubsonic app, 2026-10-16): it makes the user choose the
direction of every playlist sync, and a pushed playlist with repeats was quietly kept once each by Cratarr, so the app
showed 19 tracks and Cratarr held 9 until the next "Replace with provider version". An app that writes a list expects
the server to hold that list. The owner: "We should not be automatically removing duplicates. This should be an
option for the user", asked when it happens.

- **A playlist holds what it is given:** every write keeps its repeats (an app's, an edit's, an Undo's); the sync no
  longer takes repeats out of the files.
- **Cratarr asks where it would make a repeat itself:**
  - adding an album to a playlist that holds some of its tracks: Add again (every track at the end), or Add only the
    others;
  - a new playlist from a queue that plays a track twice: Keep repeats, or Each track once.
  Membership asks (the circle on a playlist chip, a track's playlists) stay what they were: on or off. Taking a track
  out takes every copy.
- **Repair → Repeated** lists the tracks a playlist holds more than once; **Remove repeats** keeps the first copy of
  each, one Undo for the lot (`DELETE /api/playlists/{id}/repeats`). Adding again is `POST /api/playlists/{id}/append`.
- **A repair never makes one:** a dead line pointed at a file the playlist already lists goes instead.
- **A server that holds each track once (Jellyfin)** is synced on the first copies (`playlist_merge::merge_once`,
  `Source::playlists_keep_repeats`): the file's repeats are put back where they were and are never read as removed
  there; a track taken out on either side goes with all its copies. A server that keeps repeats is merged copy by copy.

**Orders that come with it (2026-10-16):** a playlist's tracks also order by album artist, year and track number (the
files' tags), play count and last played (the owner's plays), and **Added**: when each entry came. Every playlist
write tells the user data what came and went (`playlists::on_write`), keyed as "the n-th copy of a line". Taking out
an earlier copy of a repeated track hands the later copy its date: only repeats are touched by that, and the order is
therefore not exact for them. Entries a playlist held before dates were kept have none and come first, in file order.
