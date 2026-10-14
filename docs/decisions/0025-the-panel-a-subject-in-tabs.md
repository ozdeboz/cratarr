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
  playlist mode the subject is a **selection of one or more playlists**, shown in **tabs: Tracks, Export, Repair,
  Sync.** Every tab is always there (a tab with nothing to do says so); the panel opens on Tracks. A crate (0024) is a
  subject of one, with Tracks only: crates never join a selection, since export and repair need fixed playlists.
- **Opening a playlist is selecting one.** A tap on a chip's name opens it: the selection becomes that playlist.
- **Select** (the Playlists page's toolbar) lets the selection grow: a tap on a chip's name ticks or unticks it. A
  ticked chip has the border of the playlist open in the panel (one state, one look); **the circle keeps adding or
  removing the playing track**, in Select too. On a roomy window the panel shows the selection as it is ticked; on a
  phone, where the panel covers the page, one ticks first and opens it with "Open 3 playlists". Cancel ends Select.
  A ⋮ beside Cancel opens a sheet for the ticked playlists: Play, Shuffle, Play next, Add to queue (their tracks one
  playlist after another, in the order ticked, each once), favourite, pin, Delete (one Undo for all).
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
- **The Repairs page goes.** Its playlist parts are the Repair tab. What is about the library's files (files that
  cannot be read, damaged tags, no tags, no length) moves to a new page, **Health**, beside Settings on every screen
  size.

## Consequences

- The tab row is the panel's one way to more about a playlist; new views about playlists become tabs, and one-tap
  actions stay in menus.
- Statistics and Related stay under the header of one playlist's Tracks for now (0023).
- Old `#/repairs` links open the Playlists page.
