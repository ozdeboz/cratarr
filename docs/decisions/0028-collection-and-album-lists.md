# 0028: Collection, and one album list

- Status: accepted
- Date: 2026-10-15
- Builds on: 0023 (one panel), 0025 (an album is a subject), 0027 (one row)

## Context

The owner asked on 2026-10-15 for an albums page beside Incoming, called Collection: the Music library's albums
(about 2,200 here), viewable in the panel and playable as Incoming's are. In the same breath: album lists in columns
as the playlist chips are, a grid of covers as an option on every album list, and an album "square" beside the row's
sizes. Incoming's list was its own markup, and its play and cover endpoints were Incoming's though they served any
album.

## Decision

- **Collection** (`#/collection`, after Incoming): each connected server's Music library albums, read by the server
  as Incoming's are, in pages of 250 (the whole library at once took Jellyfin 25 s), and kept, refreshed every quarter
  of an hour in the background (a page never waits for a refresh); the page orders them by Artist (a leading "The" aside),
  Title, Year or Added, the viewer's choice.
- **One album list** (`lib/AlbumShelf.svelte`) for Incoming and Collection: the filter, the order where the page has
  one, Rows or Covers, and Size, in columns (`lib/zoom.ts`, shared with the playlist chips). A tap views the album, a
  button plays it, a long-press opens the page's menu (Incoming's) or View and Play. Its rows are Row's extra large.
- **Tiles** (`lib/Tile.svelte`): a primitive beside Row rather than a size of it, since its layout is vertical (picture
  on top, text under it, the action over the picture); its size is the column the page's Size makes.
- **Album endpoints are an album's, not Incoming's:** `/api/albums/{source}/{album}/cover` and `.../play` (they work for
  any album of a connected server); `/api/incoming/cover` and `/api/incoming/play` are gone, with no redirect (no old
  links exist). Reading a library's albums with their details is one piece on the server (`app/shelf.rs`), used by
  both lists, each looking only at its own catalogue folders.
- The toolbar's Size and switches are components (`lib/SizeControl.svelte`, `lib/Seg.svelte`), used by Playlists too.

## Consequences

- A long list (2,000 albums) is rendered whole; the browser skips laying out what is off screen
  (`content-visibility: auto`), and covers load as they come into view.
- Collection has no actions on the library itself (no Keep, Replace or Delete): it shows and plays.
