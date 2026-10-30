# 0082: Classical as a kind of record, and a track's Other versions

- Status: accepted (the owner, 2026-10-30, for 0.10)
- Date: 2026-10-30
- Builds on: 0081 (release types), 0061 (the filter), 0024 (crates), 0072 (one search)

## Context

The library holds classical records under the old local convention of the composer as album artist (Brahms has 72
records, Wagner, Tchaikovsky and Beethoven many), and others under their performers (Arrau, Schiff). A classical
record is none of Album, EP or Single in a useful sense, a composer is spelt several ways, and a search for Brahms
missed the records where he is not the album artist. A genre tag alone says little: Moby's Reprise, ELO's Time, Yann
Tiersen, Pantha du Prince and Aphex Twin's Syro all carry a classical genre somewhere.

## Decision

The owner's rules:

- **Classical is a kind**, secondary like Compilation: a record can be an Album and Classical. Other is the rest and
  never holds a classical record. Modern classical counts, composers playing their own work too (Richter, Frahm,
  Arnalds). Display only: the folders on disk, Autoplay and the radio do not change (similarity decides there).
- **A genre gate and one confirmation.** The gate: a genre among Classical, Opera, Modern Classical, Contemporary
  Classical, Neo-Classical, Baroque, Romantic, Symphony, Concerto, Chamber Music and Art Song, in any case, never
  Indian Classical; from the files' tags, MusicBrainz's genres, or Discogs' genres and styles through a verified link
  only (the MusicBrainz release's or release group's url-rel to Discogs, never a name search). The confirmation, any
  one of: (a) MusicBrainz work composers on the release's recordings, with performer credits on the release or its
  recordings; (b) a composer of its works, or its album artist, is a composer confirmed classical on another record
  (by MusicBrainz id, else by the loose name); (c) a mark by hand. The tag alone is never enough.
- **Marks** "classical" and "not classical", per record, library-wide; they outlast every pass and win over the rule.
- **Composers** keyed by MusicBrainz artist, so every spelling is one; a record under each of its composers.
  Performers as a second line: conductor, orchestra, soloists, then the others. With no MusicBrainz data, the album
  artist is the composer, under (b) or (c) only, keyed `name:<loose name>`. Keys stay stable; a `name:` key that later
  gains a MusicBrainz id keeps leading there.
- **The primary view**, on by default: each record in one kind, Classical > Compilation > Album/EP/Single > Other, so
  the kinds add up to the records.
- **Search** finds a record by its composers, and its results say them.
- **Other versions, phase 1**: each track's MusicBrainz work and the work's composers, from the same release lookup;
  a track's other versions are the library's other tracks of that work, whatever the genre (standards, covers,
  classical works, remixes MusicBrainz links). A track without a work shows no action.

Design choices made here:

- **The data** is the user data's, beside the release types: `mb_releases` (per release, JSON: its genres, Discogs
  links, performers in order, whether someone performs, its tracks' works), `mb_works`, `artist_person` (MusicBrainz's
  "is person" and the artist's Discogs link), `discogs_names`, `discogs_link_genres`, `classical_marks` (by the
  record's Music-form id, as its tags) and `composer_keys` (every key given out, and where it leads now). New tables
  only.
- **The lookup** is the owner's, with `artist-rels` added (the works' composers are work-level artist relations, which
  come only with it) and `url-rels+genres+release-groups+release-group-level-rels` (the genres and the Discogs links),
  one call per release still; should MusicBrainz refuse the long form, the owner's plain one is asked.
- **The passes** run in MusicBrainz's background asker in turn with the release types (one asker, its pace of one
  question every 2 s): first the releases (200 a pass, each kept as it comes, asked again after 180 days, a release
  MusicBrainz did not know after 30), then the artists of the records whose genres open the gate (their album artists
  and works' composers: "is person" and the Discogs link). Discogs, in its own six-hourly rounds and only through those
  links: the linked releases' genres where the files' and MusicBrainz's leave the gate shut, and the linked artists'
  real names and aliases, 300 of each a round. The related artists' lookup keeps "is person" too.
- **Performer credits** for (a): a performing relation (conductor, orchestra, instrument, vocal, performer), or a
  credited artist who composed none of the release's works. An artist credit only naming the works' composers
  performs nothing by that, so a pop album whose songs link to its artist's own works stays out; a composer at the
  piano counts through the relation.
- **(b) in turn**: a record confirmed by (b) confirms its composers for others, until none is added.
- **Names to MusicBrainz artists**: an album artist with no MusicBrainz id is a composer when their loose name, or
  one of their Discogs names, is the name, sort name or family name of exactly one MusicBrainz composer of the library
  ("Bartók Béla" is Béla Bartók by his sort name); a performance name counts as its person.
- **Made in memory** (`app::classical`), from the library and the user data: kept until the library is built again or
  a mark changes, and at most a minute behind the passes. No library rebuild for a mark.
- **Classical everywhere a type is filtered**: the crates' facts carry it among a file's release types (every crate,
  the radio's and Autoplay's crates, and every list's filter: artists, tracks, playlists, an album's, an artist's),
  read again at once for a mark. The Library's records are weighed by the record, on their kinds as listed, in the
  primary view unless the filter's `all-kinds` switch is on (the page's: off by default, the server's precedence on),
  and the Library's Type axis counts records.
- **Shown on** the Library's, the Collection's, Home's and an artist's albums (`classical`, `classical_mark`,
  `composers`, `performers`; `classical` for every album of Cratarr's own library, none for another server's, which
  says nothing of it); a classical record's composers and performers only.
- **Search** finds composers in memory, not in the full-text index: the records of a composer whose name holds the
  words come after those the words name, marked as found by their composer.
- **Not given Classical**: OpenSubsonic's `releaseTypes` (MusicBrainz's release types, which Subsonic apps group by;
  Classical is none of them), and Discover's, Spotify's and the requests' albums (outside the library; nothing confirms
  them).

## The API

- `GET /api/albums` (and the Collection's, Home's, an artist's albums): `classical`, `classical_mark?`, `composers?`
  (`[{key, name}]`), `performers?`.
- `POST /api/filter`: `Classical` a value of the `type` axis; the Library's `all-kinds` switch.
- `GET /api/classical/composers` → `[{key, name, sort_name, records, photo?}]`;
  `GET /api/classical/composers/{key}` → `{key, name, records}`, 404 for none.
- `PUT /api/albums/{source}/{id}/classical` `{mark}` (administrators') → `{classical, classical_mark}`; the pages
  showing albums are told.
- `GET /api/tracks/{source}/{id}/versions` → `{work, composers, versions}`, 404 without a work; an album's tracks
  carry `has_versions: true` where another version exists.
- `GET /api/search`: `composers` on a classical record's, or its track's, result.

## Consequences

- A record of the old convention is classical once its composer is confirmed on any record with MusicBrainz data.
- A record without MusicBrainz data or a confirmed composer needs a mark.
- The first pass over a library of a few thousand releases takes a few hours at MusicBrainz's pace.
