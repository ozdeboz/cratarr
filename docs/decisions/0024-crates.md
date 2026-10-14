# 0024: Crates

- Status: accepted
- Date: 2026-10-14
- Builds on: 0011 (the catalogue), 0013 (Incoming), 0023 (the panel)

## Context

After Incoming (decision 0013) an album is in Music, but in no playlist: putting its tracks where they belong is the
second step, and nothing in Cratarr helps with it. The predecessor (M3U Library Manager, its Curation page) had
"Collect": the favourite tracks in no playlist, written to a frozen `Unplaylisted_<time>.m3u` and opened in a player to
sort by ear. It had no other selection, the snapshots piled up and went stale as one sorted.

## Decision

The owner decided (2026-10-14):

- **A page, Crates,** between Incoming and Playlists: Incoming · Crates · Playlists · Insights · Repairs (Settings
  apart).
- **A crate is a dynamic playlist:** a cross-section of the library along its axes (playlist membership, favourites,
  later BPM, genre, year, plays…), worked out when it is looked at. A track sorted into a playlist leaves "Unlisted" by
  itself. Only crates are dynamic; playlists never are.
- **Permanent crates,** always there, not renamed or deleted:
  - **Favourites:** the tracks the account Cratarr plays as (decision 0022's "Plays as") has as favourites.
  - **Unlisted:** the tracks in Music in no playlist.
  - **Unlisted favourites:** both (the predecessor's Collect).
  Only Music counts (not Incoming).
- **In a crate the newest come first** (when the catalogue first saw the track; a kept Incoming album keeps the time it
  arrived there), so what was just kept is on top.
- **Opened like a playlist:** the page shows the crates as chips with their live counts; a crate opens in the panel
  (decision 0023) as a playlist does: Play, Shuffle, Play next, Add to queue, a tap plays from a track, the hearts and the
  track menu; no edit mode (a crate changes by sorting its tracks into playlists). Long crates come in pages.
- **Make playlist:** a crate's tracks at that moment saved as a new, ordinary playlist (a snapshot, named when made),
  which then opens in the panel; the crate stays.
- **Later, not in the first version:** crates of one's own, composed from the axes (conditions combined, each one
  negatable), starting with membership, favourite, genre and year; BPM needs a source first (a file's tag, or analysis).

## Consequences

- The server works out a crate from the catalogue (Music's files, their first-seen time), the playlists' lines and the
  media server's favourites; a crate's rows are the playlist view's rows, so the panel shows them the same way.
- The phone's top bar has one page more: Repairs moves under More.
- `docs/ui-conventions.md` describes the page.
