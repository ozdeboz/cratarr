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
