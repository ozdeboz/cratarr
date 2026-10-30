# 0042: Columns where there is room, and a sidebar that folds

- Status: accepted
- Date: 2026-10-17

## Context

On a wide window the lists use little of the width: a page shows its rows several to a line (the Playlists page's
four), and a track list in a panel opened over the page is one narrow row per line. The owner asked how far a
table-like view could go, as Spotify's desktop app, Feishin and MusicBee show one: one line per item, its details in
columns. Pages and panels can each fill the screen, so every list should get the same treatment, built once
(decision 0027: one row primitive, `lib/Row`, and one list header, `lib/ListHead`).

## Decision

**Columns, by priority, as the list's own width allows.** Every list built from the row can show columns between its
title and its end: a page in its new Table layout, and the panel's track lists whenever they are wide.

- Each kind of list has its columns in an order of priority: tracks (album, tempo, plays, last played, added, year,
  album artist, genre, format), albums (year, tracks, length, added, plays, last played, format, genre), playlists (tracks, length,
  genres, last played, changed). (Crates: no table, the owner 2026-10-17.)
- The list measures its own width (not the window's: a panel beside the page and one over it differ) and shows as many
  columns from the top of the order as fit, each at least its own width (tempo needs little, album much).
- The order is the owner's: View lists a kind's columns, dragged into the order wanted, remembered per kind. The first
  one appears first.
- Under the list's header (`ListHead`) a head row names the columns shown. It is a row itself (`Row` with `head`), so
  its names line up with the values. A tap on one orders the list by it (as its order sheet does); a second tap turns
  it round. Counts and dates start with the most and newest.
- The room a row has beyond its columns' widths goes to the title and to the columns of names (album, album artist),
  the title taking twice a column's share; numbers and dates keep their width.
- A list shows only the columns its rows carry values for (a server's playlist has no plays).
- A phone keeps today's rows: no column fits.

**A Table layout for the pages.** Next to Rows and Covers (decision 0028), Table shows one item per line over the full
width, with columns. Rows and Covers stay as they are. Built first for the album pages (Library, Downloads; 2026-10-17): year,
tracks, added, plays, last played, format and rating, a smaller cover whose Play shows on the line a mouse points at;
a table has no sizes. The playlists' table follows once the server lists their length, genres and dates. Crates stay
big cards only, no table (the owner, 2026-10-17).

**An edge's handle is the edge's line.** The line between page and panel, and the navigation's edge, are each 1 px
wide; what is on either side fills right up to it. The handle in the middle, and the room to grab it, reach over both
sides.

**The navigation folds (wide windows).** Its edge is the same handle as the panel's: a tap folds it to a strip of icons
or unfolds it, a drag follows the pointer and snaps to one of the two on release (never a width between), as Spotify's
library and VS Code's sidebar do. Remembered per browser. Folded:
- a tap on an icon goes there at once;
- hovering over the strip, or a tap on the strip outside an icon, shows the whole navigation over the page without
  pushing it aside; it folds again when the pointer leaves or after a tap elsewhere.
Unfolded it takes its room beside the page, as now.

## Consequences

- One column system in `Row` and `ListHead`: every list gets it at once, the queue sheet's rows to follow once they
  are built from the row.
- Columns show what a list's rows already carry; one that needs more from the server comes with its own change.
- The folded strip gives a page about 180 px more.
