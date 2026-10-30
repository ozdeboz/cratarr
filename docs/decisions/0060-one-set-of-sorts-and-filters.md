# 0060: One set of sorts and filters per kind of item

- Status: accepted (the owner, 2026-10-23: "This is also entirely scattered and barely complete"; "Yes, please" to
  writing it up; the tables: "Build as proposed")
- Date: 2026-10-23
- Builds on: 0058 (one menu system), 0025 (playlist views and orders), 0035 (tempo), 0042 (columns), 0024 (crates)

## Context

Sorting and filtering grew page by page. The same fact has different names ("Plays" for an album, "Play count" for a
track). A crate's tracks cannot sort by plays or year though a playlist's can; the Crates page sorts only "As made" and
"Name" though its cards show size, length and what is unplayed. "Favourites only" filters albums and playlists but not a
list's tracks; "Not played yet" and "4 stars and up" exist only on the Library; a crate's favourites switches sit in its
header, apart from the Filter sheet. Pages remember their sort; the panel forgets its sort and filters each time.
Order and direction are set in two places (the list's bar and the View sheet).

## Decision

### Where each control lives

- **The list's bar:** order, direction, the count, Select. Order and direction only here.
- **The View sheet:** how the list looks: layout, size, dates. No order or direction.
- **The Filter sheet:** every filter of the list, in the order of the table below; then the servers and libraries
  (as now, remembered everywhere).

### One name per fact

Plays (not "Play count"), Last played, Added (an album: to the library; a list's entry: to the list), Length, Tempo,
Year, Rating (albums; tracks have none), Title, Artist, Album artist, Album, Track number. A list's own order is named
by what it is: Playlist order, Track number (an album), Newest first (a crate), Newest album first (an artist).

### Sorts per kind of item

★ = new for at least one list of that kind.

| Kind | Sorts |
|---|---|
| Album (Library, Downloads, device albums) | Artist · Title · Year · Added · Plays · Last played · Rating · ★Quality (format and bit rate) |
| Track (a playlist's, crate's, album's, artist's; device tracks) | Own order · Title · Artist · Album artist · Album · Year · Track number · Length · Tempo · Plays · Last played · Added — every list of tracks offers all its rows know (★ a crate's and an album's gain Album artist, Year, Plays, Last played, Added) |
| Playlist | Name · Tracks · Playing first (Ticked first in Add to playlist) · ★Missing |
| Crate | As made · Name · ★Tracks · ★Length · ★Unplayed · ★New this week |
| Artist (device artists) | ★Name · ★Albums · ★Tracks |

Downloads and the device's own music offer only what is known offline (no Plays, Last played or Rating where the device
does not keep them).

### Filters per kind of item

| Kind | Filters |
|---|---|
| Album | Favourites only · Not played yet · 4 stars and up · Upgrades waiting (Cratarr's library) · ★Lossless only |
| Track | ★Favourites only · ★Not played yet · Hide missing · Tempo range |
| Playlist | Favourites only · ★Mine only (not shared by others) |
| Crate | — (a crate's own Favourites and Favourite albums settings stay its settings, visible in its header: the owner, 2026-10-23) |

### What is remembered

Every list remembers its order, direction and filters, per kind of list: the Library, Downloads, Playlists, Crates, the
device's pages, and in the panel one each for a playlist, a crate, an album and an artist (per kind of list, not per
list: the owner, 2026-10-23).

## Consequences

- One field table and one filter table in code (`lib/pageView.svelte.ts`), read by the list bars, the Filter sheet,
  the table columns (0042) and the View sheet; no page keeps its own list of fields.
- The panel's sort and filters move from component state into remembered settings.
- A crate's header keeps its two favourites switches.

## Amendment (2026-10-24): in the panel, a playlist's filter only

The owner, asked why an album's panel has a filter: "Fine, keep it on playlists." In the panel only a playlist's tracks
are filtered (Favourites only, Not played yet, Hide missing, and 0061's conditions). An album is too short and alike to
filter, a crate is a filter on the library already (its rules are where it narrows), and an artist's lists are theirs.
Their pill keeps View; a filter remembered for them from before goes by nothing.
