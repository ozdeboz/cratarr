# 0086: Classical's works and movements

- Status: accepted (the owner's three choices, 2026-10-30, for 0.12; the server and the page on trunk, 2026-10-30).
  Works in global search: a `work` search kind on `/api/search` (top-level works, by title, catalogue and composer),
  on trunk 2026-10-30.
- Date: 2026-10-30
- Builds on: 0082 (Classical as a kind of record, Other versions phase 1), 0085 (offline first: capabilities)

## Context

0.10 made Classical a kind of record that opens on its composers, a composer opening their records (0082), and gave a
track its Other versions through its MusicBrainz work. A composer is still only a shelf of records: their works, the
unit a listener thinks in (the Goldberg Variations, the Fifth), appear nowhere, and an album of three symphonies is a
flat list of twelve long titles, each repeating its symphony's name before the movement's.

The data is half there. 0082's lookup keeps each track's MusicBrainz work and the work's composers (`mb_works`), but
not which work a movement is part of: a movement is a work of its own in MusicBrainz, its symphony the parent, a
work-to-work relation the lookup does not ask for yet.

## Decision

The owner's choices (2026-10-30, the question tool):

- **A composer's page: works first, records below.** Their works, the most recorded first, each with how many of
  its recordings the library holds; then their records as in 0.10. A work opens its recordings.
- **A work: its recordings, each playable.** One row per recording: its performers (conductor, ensemble, soloists,
  as 0082 orders them), album, year, cover and length. A tap plays that recording's movements in the work's order;
  ⋮ opens its album. The recording chosen lists its movements under it.
- **An album: a work's heading over its movements.** A heading with the work's name and Play for the whole work, above
  its movements, set in and named by the movement only ("I. Allegro"); tracks of no work as they are.

Design choices made here:

- **What counts as a work's movements on an album:** two or more tracks in a row whose works have the same parent. A
  single movement, a work with no parent known yet, or a track of no work stays a plain row; a work's movements split
  by another track get a heading over each run. So a heading is never half there while MusicBrainz's parts come in.
- **A movement's name:** the track's title after the work's name (": ", " - ", "–", "—" or ". " after it), its
  numbering as the tags have it (roman, arabic or none); else its part's title after the work's name; else the track's
  title unchanged. The work's name counts when the head is the same words, or a short form of them from the start that
  keeps the work's number ("Symphony No. 5" for "Symphony No. 5 in C minor, Op. 67", never "Symphony No").
- **Grouped only in the album's own order.** Sorted (by length, say) or filtered, the album is a plain list again.
- **Playing:** a recording's tracks in the order of its work's parts (a track whose part is not known keeps its place
  after them), the whole work from a heading in the album's order; both through the one way to play (`kind: tracks`).
- **Where:** Cratarr's own library. Another server's albums say nothing of works.
- **The capability `works`** (0085): the server lists it once it serves the endpoints below. Without it the composer's
  page is its records alone and albums are plain lists, as in 0.11; nothing is offered that the server cannot answer.

## The API (a5's, written 2026-10-30)

- `GET /api/classical/composers/{key}/works` → `[{work, title, key?, catalogue?, parts, recordings}]`, beside 0.10's
  composer routes: the composer's top-level works (`work` its MusicBrainz id; `key` its musical key; `catalogue` its
  catalogue number), with how many parts it has and how many recordings the library holds; most recorded first, then
  by title (numbers by value); 404 when no classical record has that composer.
- `GET /api/works/{mbid}` → `{title, composers: [{key, name}], parent?: {mbid, title}, parts: [{mbid, title,
  number}], recordings: [{album_id, album, performers: [..], year, tracks: [{id, part?, duration_secs?}]}]}`; 404 for a
  work the library does not hold. `duration_secs` is the page's ask, for a recording's length. Recordings oldest first, tracks in
  movement order; a movement's mbid opens its parent.
- An album's rows (`GET` of an album's tracks) gain `work?: {mbid, title, parent?: {mbid, title}, part_number?}`, on
  every album of Cratarr's library where a work is known, not only Classical ones (the grouping rule holds the same).
- The data: MusicBrainz's per-release question already carries each work's parent (its "parts" relation, with its
  ordering), so no new question is asked; releases stored before are asked again at the normal pace, about 2 to 4
  hours for the library, which the grouping above copes with. One level of parent only: an opera aria's parent is its
  act, so an opera's acts show as works of their own on the composer's page until a per-work question adds the level
  above.

## Consequences

- A composer's page leads with what was composed, a work gathers its recordings across albums, and an album of
  symphonies reads as symphonies.
- Until a work's parts are known, its movements show as before, then grouped once they are; nothing to do by hand.
- A recording's length needs `duration_secs` on its tracks; without it the row shows no length.
