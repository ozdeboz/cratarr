# 0061: A filter is an unsaved crate on the list in view, any or all of its tracks

- Status: accepted (the owner, 2026-10-23: "We should be able to filter on genre, for example, and possibly many other
  features. Currently, the only way to see a cross-section of genre is by creating a crate, and that's not very
  efficient"; "We should make our filter engine allow for any or all. In the case of nested objects, such as albums
  containing tracks"; "filter should be remembered like sort")
- Date: 2026-10-23
- Builds on: 0024 (crates: conditions on the library's axes), 0060 (sorts and filters per kind of item), 0059 (rules run
  where the library is), 0054 (AudioMuse's mood, energy, danceability and style)

## Context

Filters were a few switches per page (Favourites only, Not played yet, …). Seeing a genre's cross-section of a list
meant making a crate. Crates already hold the engine: conditions on ten axes (favourite, favourite album, in playlist,
genre, year, BPM, mood, energy, danceability, style), each negatable, all of them holding, any of several values within
one.

## Decision

- **A filter is a crate's conditions, unsaved, on the list in view** (the Library's albums, Playlists, Crates' tracks, a
  playlist's, crate's, album's or artist's tracks, the device's own music). A crate is the same conditions, saved, on the
  whole library. One engine, one builder: the Filter sheet draws the crate builder's controls (genre, mood and style as
  chips, the most common in this list first, with their counts; year, BPM, energy and danceability as ranges; favourite;
  in playlist). The panel's tempo range is the BPM condition.
- **Any or all, per condition, for items that hold tracks** (an album, an artist, a playlist): Any, at least one of its
  tracks matches; All, every one of its tracks that has a value for it matches (a track with none, not yet analysed,
  does not count against All). Any is the default; the switch shows only where the items hold tracks.
- **Remembered like sort** (0060): per kind of list.
- **How it looks** (the owner, 2026-10-23, of four mockups: "No, we keep the list clean. So C seems to be the most
  logical and clean way"): a card per condition, in the Filter sheet and in a crate's editor alike. Folded, a card is
  one line: its name and what it is set to, a switch that mutes it without removing it (dimmed), and a chevron;
  unfolded, its whole control (values as chips with their counts, a range, Tempo's spread), Not (a pill, red when on)
  and Remove, and last, where the items hold tracks, its own line: "Matching any | all tracks in each album" (the owner:
  beside the switch, Any and All read as being about the chosen values). Folded, All is said in the summary ("…, all
  tracks"). "+ Condition" adds one. Under the cards the kind's own switches (no Clear or Save as crate buttons:
  the owner, 2026-10-23, "two buttons I did not ask for"); Save as crate is an icon at the title's end instead, a crate
  with a plus ("I do like the save as crate option"), turning the conditions that apply into a new crate; the sheet says how many the filter lets through ("312 of 2,861"). The list itself stays clean: only the funnel shows
  how many filters are on.
- **Where it runs** (0059): the server's library on the server (it knows the genres and AudioMuse's values); the device's
  own music on the device, with the conditions it has facts for (genre, year, favourite), the others not offered there.
- 0060's filter table becomes: these conditions for every kind, plus each kind's own switches (Upgrades waiting, Hide
  missing, Mine only, Lossless only).

## Consequences

- The crates' condition engine takes a scope (a list's tracks, grouped by album, artist or playlist) and a quantifier per
  condition, besides the whole library.
- One filter state per kind of list in the remembered settings, beside its sort.
- The Filter sheet becomes the builder's conditions above each kind's own switches; the crate editor draws the same
  cards.

## Built (2026-10-23): the engine's semantics

- **A condition** carries `enabled` (default on; a muted one is ignored, kept to switch back on) and `quantifier`
  (`any`, the default, or `all`). Saved crates without them load unchanged and are saved as they were.
- **Any:** at least one of the item's tracks holds the condition, as a crate tests a track (negation included).
- **All:** every one of its tracks that has a value on the axis matches; a track without one (not analysed, no genre
  tag) does not count. Not negated, at least one track must have a value: an album none of whose tracks is analysed is
  not "all high energy". Negated, it is "none of the tracks with a value matches", which an item without values passes,
  as a single track without a value passes a negated condition. A track alone is an item of one: Any and All agree.
- Every condition that is on must hold (AND), each with its own quantifier; none on, everything passes.
- **One endpoint**, `POST /api/filter {list, conditions, without}`: the list (the Library's albums, the playlists, a
  playlist's, a crate's, an album's or an artist's tracks) answers the keys of the items that pass, `passed` of
  `total`, and its axes over all its items (values with counts, most common first; the ranges' span; the tempo
  spread). A crate's rows are paged, so the filter on a crate also travels with its rows, tempo, Play and export
  (`filter`), and its keys are not sent.
