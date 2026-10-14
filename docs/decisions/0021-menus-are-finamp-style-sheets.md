# 0021: Menus are bottom sheets, like Finamp's track menu

- Status: accepted
- Date: 2026-10-14
- Builds on: 0018 (the player and its chrome), 0019 (the track menu); changes "Gestures" in `docs/ui-conventions.md`

## Context

Cratarr's menus were small popovers anchored to the item (the playlist menu on Playlists and Insights, "New
playlist", the album menu on Incoming, and the player's track menu). Finamp's track menu, read in its source
(`lib/menus/track_menu.dart`, `lib/components/themed_bottom_sheet.dart`,
`lib/menus/components/menu_item_info_header.dart`, `.../playback_actions/*`, `.../menuEntries/menu_entry.dart`,
branch `redesign`, 2026-10-14), is a different kind of menu: a bottom sheet with the item at its top.

## Decision

The owner chose (2026-10-14): **every menu becomes a bottom sheet modelled on Finamp's track menu**, one component
(`lib/MenuSheet.svelte`) on every page.

- **The sheet:** rises from the bottom (about 250 ms, decelerating), at most 500 px wide and centred, top corners
  rounded 20 px, a drag handle (40 × 3.5 px). A dim backdrop (black, 54 %); a tap on it, Esc, Back or dragging the
  sheet down closes it. Its background is the surface with the item's cover blurred under a veil when there is one,
  and its accent comes from that cover (as the player's, decision 0018).
- **The header:** a card 12 px in from the sides, 140 px high, corners 12 px, a translucent fill (black 25 %, light
  theme white 15 %); the item's image at 140 × 140 on its left (the card rounds its corners); the title (18 px, up to
  two lines), then small chips (artist, album; for a playlist its track count); for a track the heart at its
  bottom right. **A playlist's image is a mosaic** of the covers of its first four albums (2 × 2), from a small
  read-only server request.
- **The action row** under it, as Finamp's playback row: 96 px high, the actions spread evenly, each a 28 px icon
  with a small light label under it (12 px), no fill. For a playlist: Play, Shuffle, Play next, Add to queue; for the
  playing track: Play next, Add to queue.
- **The entries:** rows 56 px high, a 24 px icon in the accent colour, the label at 16 px; no dividers, no section
  headings. **Destructive rows stay Cratarr's way:** last, and red (where Finamp shows them like any other row).
- **Steps inside a menu** (Rename, Spoken as…, Sync to…, the delete confirmation, the playlists to add a track to,
  New playlist from the queue) replace the entries inside the same sheet, the sheet growing or shrinking smoothly
  (Finamp's inline panels); a back arrow returns to the entries.
- **The playlists for a track** (Finamp's playlist menu): rows as cards, corners 12 px, filled with the accent at
  30 % when the track is in the playlist and 10 % when not, a mark at the right; a tap adds or removes.

## Consequences

- One menu component instead of the anchored popover; `lib/Popover.svelte` goes once nothing uses it.
- A menu no longer points at the item it belongs to; the header says what it is about instead.
- The server gains a read-only request for a playlist's first album covers.
