# 0007: Playlists are exported on demand, as a download

- Status: accepted
- Date: 2026-10-12

## Context

People want to take playlists elsewhere: a USB stick, a car, a phone, someone else, optionally with the music.
Three concerns were separated: the data layer (the managed `.m3u` files in the music folder, unchanged), protecting
it (left to whoever runs Cratarr; an automatic copy was considered and dropped), and export (this decision).
Cratarr runs with a small memory budget (64 MB on the reference install), and playlists can hold thousands of
tracks (tens of GB).

## Decision

- Export starts on Manage → Playlists: tick one or several playlists, then Export.
- The dialog offers three independent options:
  - Include tracks.
  - All tracks in one folder: files named `Artist - Title.ext` instead of the library's folders; a name clash
    gets ` (2)`.
  - Combine into one playlist: a typed name, prefilled from the selection; each track appears once, at its first
    position.
- Playlists are written as plain `.m3u`, one path per line. With tracks included, the paths point at the bundled
  files; without, they are the managed files' lines as they are.
- One playlist without tracks downloads as a `.m3u`; anything else as one `.zip` (stored, not compressed: audio is
  already compressed). The download is streamed, so memory use does not depend on its size; ZIP64 covers files
  over 4 GB.
- Lines whose file is missing are left out and listed in a report file in the bundle. The dialog says how many
  before starting, together with the track count and total size.
- Only files inside the playlist folder are ever read, by the same rules as everywhere else; the request names
  playlists, never paths.

## Consequences

- No server folders or settings are involved; it works for anyone.
- A browser download of tens of GB takes time; the browser shows its progress, and closing it cancels.
- Plugin seams (other playlist formats, other delivery targets) are left until there is a second of either.

## Amendment (2026-10-14): where it starts, what it takes, what the lines say

The owner settled what changed since: the Manage page is gone (0.2.6), playlists moved into `Music/.playlists`
(decision 0014), and crates arrived (decision 0024).

- **Where it starts:** the panel's **Export** tab, for the playlists selected (decision 0025: one or more, ticked
  with the Playlists page's Select); **Export…** in a playlist's menu opens that tab for it.
- **Playlists only.** An export needs a fixed list, so a crate, which changes with the library, is not exported:
  its **Make playlist** (decision 0024) makes one first.
- **The lines are rewritten,** with or without tracks: the managed files' `../` lines mean nothing outside the
  server, so without tracks each line is the file's path relative to the music folder (`Artist - Album/01 Track.flac`),
  as the playlists were before 0014. With tracks the lines point at the bundled files, as above.
- The tab's count, size and missing files come from the catalogue (decision 0008), so they show before starting
  without asking the media server.
- It is called **Export** in the interface.
