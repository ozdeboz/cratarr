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
- **Crates of one's own** (owner, 2026-10-14, issue by issue):
  1. **Built in a third panel mode,** the crate builder, beside the player and the playlist/crate view: "New crate" (on
     the Crates page) opens it empty, a crate's edit button opens it on that crate; saving shows the crate.
  2. **Kept on the server, in one file** (`crates.json` in Cratarr's data folder, the previous version kept beside it
     for undo), so every device has the same crates.
  3. **Rules:** a crate is a list of conditions, all of which must hold, each one negatable. OR within an axis: a
     condition takes several values (genre is any of rock, jazz) or a range (year 1990–1999). Groups (OR across axes)
     later; the stored format carries a version for that. The permanent crates are definitions in the same model
     (Favourites: favourite is yes; Unlisted: NOT in any playlist; Unlisted favourites: both).
  4. **A generic axis registry:** each axis (on the server) declares its kind (yes/no, range, values, membership), its
     domain (the genres there are, the years' span) and how it is evaluated; the server lists them and the builder draws
     its controls from the list, so a new axis needs no builder work. First axes: favourite, playlist membership, genre,
     year, BPM.
  5. **Freshness:** library axes (the catalogue, the playlists) are always current; axes from the media server
     (favourites; later plays) are kept about a minute and dropped when Cratarr itself changes them.
  6. **Live preview:** while building, each change (after ~300 ms) shows the unsaved crate's count and first tracks.
  7. **BPM from the files' BPM tag;** an outside analysis job (not in Cratarr's 64 MB container) can later fill the gaps
     through the same axis.
  8. **Identity:** a crate has an id that never changes (the panel's address, links); its name is unique among crates
     and is not a permanent crate's name (it may equal a playlist's).
  9. **The draft** has its own address (`?open=crate-new`, `?open=crate-edit:<id>`) and is kept in the browser while
     open: it survives the player over it, a page change, turning the tablet and a reload; Save or Cancel ends it.

## Consequences

- The server works out a crate from the catalogue (Music's files, their first-seen time), the playlists' lines and the
  media server's favourites; a crate's rows are the playlist view's rows, so the panel shows them the same way.
- The phone's top bar has one page more: Repairs moves under More.
- `docs/ui-conventions.md` describes the page.
