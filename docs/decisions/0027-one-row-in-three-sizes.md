# 0027: One row in three sizes

- Status: accepted
- Date: 2026-10-14
- Builds on: 0021 (Finamp-style sheets), 0023 (one panel), 0025 (the panel's tabs)

## Context

Lists had grown one by one, each its own way: the Playlists page's chips (Finamp's add-to-playlist tiles), the crate
chips styled after them, the Export tab's own playlist rows, the Info tab's links, the Repair tab's and Health's file
lines with borders, the Sync tab's target lines, and the track rows. The same playlist looked different on every tab,
and the folding sections differed on Health and in a selection's Tracks. The owner asked on 2026-10-14 for one
component per kind of thing, built from shared primitives, in three sizes with one layout.

## Decision

- **One row primitive** (`lib/Row.svelte`) with one layout, left to right: lead, title with marks (and a second line),
  count and flags, action. **Sizes:** large in the panel's Tracks, Info, Repair and Sync tabs (and wherever tracks are
  listed: a track is a large row with its cover); medium on the pages (Playlists, Crates); small in the panel's Export
  tab. Small and medium are Finamp's add-to-playlist tile; large is Finamp's list tile.
- **One component per kind of thing** on top of it: a playlist (`PlaylistItem`), a library file (`FileItem`), a
  track's cover (`TrackCover`); a crate is a medium row on the Crates page. Where a thing is listed decides its size,
  its second line and its action, never how its name, marks and count look.
- **One section primitive** (`lib/Section.svelte`) for a list's folding heading with its count and one line on what it
  is.
- The medium row is the Playlists page's chip unchanged (to the pixel); the other places took the row's looks.

## Consequences

- A new list uses Row (or the thing's component) instead of its own markup; the conventions are in
  docs/ui-conventions.md → Rows.
- Not yet on it: the queue sheet's tracks (decision 0019's Finamp port), Incoming's albums, Insights' lists and
  Settings' libraries; they move when they are next changed.
