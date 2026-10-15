# 0030: Cratarr serves the OpenSubsonic API

- Status: accepted
- Date: 2026-10-15
- Builds on: 0005 (sources), 0008 (catalogue), 0011 (track ids), 0029 (AGPL)

## Context

Cratarr manages playlists alongside a media server (Jellyfin, then Navidrome in a spike). The owner wants no separate
music server: Cratarr serves the library itself, through the OpenSubsonic API, so any Subsonic app can use it. Cratarr already has most of what a server needs: the catalogue (every file, its tags, stable
track ids that survive moves), original-file streaming and folder covers. The owner asked to copy proven code and
patterns as far as possible rather than reinvent; with the AGPL (0029) the copyleft servers can be used.

What was studied (2026-10-15):

- **nghe** (Rust, AGPL-3.0): an OpenSubsonic server. Its API types and envelope are usable, but every crate needs
  nightly Rust, every id is a UUID, its data layer is PostgreSQL throughout, and it returns non-200 statuses on errors.
- **Navidrome** (Go, GPL-3.0, SQLite): the server most clients are tested against; the source of the library and
  performance patterns below.
- **gonic** (Go, GPL-3.0): albums are folders; simpler pieces (LIKE search, counts at query time, a plain cover cache)
  that suffice for a library of 24,000 tracks.

## Decision

Cratarr serves the OpenSubsonic API at `/rest/*` on its own port. Code and patterns are taken as follows; copied or
translated code keeps a notice naming its origin, copyright holder and licence, and says what was changed.

**Data (patterns from Navidrome, translated):**

- A derived **library database** (`library.db`), rebuilt from the catalogue: songs, albums, artists and a search index.
  The catalogue is read through a read-only connection of its own (a full read takes seconds and must not hold up
  the catalogue), and only when a cheap summary of its files changed; it stays the truth about files and tags.
  Deleting `library.db` costs a rebuild.
- **One album per folder** (Cratarr's layout, decision 0016; gonic's model): songs of a folder whose tags disagree
  (a date on some tracks only) would otherwise split it, as Navidrome's grouping does (a library of 2,736 album
  folders gave 2,754 albums, one folder five). The folder's album id is the one most of its songs' tags give.
- **Album aggregates on the album row** (track count, length, size, years, genres, discs, added = oldest track), and
  album and artist rows recomputed only for what changed since the last pass.
- **Ids:** songs `tr-<catalogue track id>` (they survive moves, decision 0011); albums and artists stable hashes in
  Navidrome's manner: the album from its MusicBrainz release id, else album artist, album and release date; the
  artist from its MusicBrainz id, else its lower-cased name. Each song also carries Navidrome's "persistent id" (from
  its MusicBrainz track id, else album, disc, track number and title), so user data finds its song again after the
  catalogue is rebuilt and its track ids renumbered.
- **User data in its own database** (`userdata.db`, never derived, kept in backups): one annotation table per user
  and item (plays, last played, rating, starred and when), as Navidrome's; play queues. Nothing is ever deleted when a
  file goes missing: missing songs and albums are flagged, not removed.
- **Search:** SQLite FTS5 (unicode61, diacritics removed), each word as `(word OR word*)`, ranked by bm25 with column
  weights, a LIKE fallback where FTS finds nothing, MusicBrainz ids matched as such.
- **Paging:** two phases (the page's row ids first, then their details joined) with covering indexes, for the full
  listings clients make (`search3` with an empty query).
- **SQLite settings:** WAL, `synchronous=NORMAL`, a busy timeout, `ANALYZE` after a rebuild.

**API (from nghe, adapted to stable Rust and string ids):**

- The request and response types and the `subsonic-response` envelope; request parsing (query string, POST form,
  `.view` suffixes); token, password and API-key login. Errors are answered with HTTP 200 and the error envelope, as
  clients expect; empty lists as `[]`; endpoints not implemented answer 501.
- Streaming of original files with ranges and ETags; no transcoding in the first version (no ffmpeg in the image).
- Covers from the folder (`cover`, `folder`, `front`), else embedded art; cover ids carry a hash of the picture so an
  image can be cached for good; resized copies in an on-disk cache with a size limit.
- `getIndexes` honours `ifModifiedSince`; the artist index ignores articles ("The", "A").

**Playlists:** Cratarr's playlists (the `.m3u` files, decision 0002) are the Subsonic playlists; a client's edit is an
edit of the file.

**Users:** logins are Cratarr's own. The classic Subsonic login needs the password itself, so passwords cannot be
hashed; they are kept as Cratarr keeps its other secrets, in an owner-only file in the data folder
(`subsonic-users.json`), not in the user data database that goes into backups. (nghe encrypts them with a key in its
configuration; a key kept in the same data folder would protect nothing.) API keys (the OpenSubsonic extension) can
follow.

## Plan

1. The library and user databases, and a one-time import of plays and favourites from Jellyfin.
2. Reading endpoints, login, streaming and covers; tried with Symfonium, Feishin and Cratarr's own Subsonic source.
3. Writing endpoints: stars, scrobbles, playlists, play queue.
4. Cratarr's own pages use its own data instead of a media server's.
5. Switch over: the tablet plays Cratarr (the APK). Home Assistant calls Cratarr's own API: Music Assistant was
   removed on 2026-10-15 (ha-infra 743e395); what Home Assistant needs beyond Subsonic (pushed player state, commands
   per player, album tags, incoming, health) is mapped in ha-infra `docs/plans/music-on-cratarr.md`.

## Consequences

- Jellyfin is no longer needed for music; Finamp (Jellyfin only) gives way to Cratarr itself or another Subsonic app.
- Cratarr needs logins, which it never had; its own pages stay as they are until sign-in is decided.
- Client quirks become Cratarr's to handle; each release is tried against the clients above.
- Not taken: nghe's PostgreSQL layer, scanner and ffmpeg transcoder; Navidrome's multi-library tables, separate
  scanner process and artwork store; a file watcher.

## Amendment (2026-10-15): importing from a server

Plan step 1's one-time import becomes a lasting feature, decided with the owner: anyone can copy an account's listening
history from a connected server (Jellyfin, or any Subsonic server such as Navidrome) into a Cratarr account, and do so
again whenever they like, later perhaps every few minutes while both servers are in use.

- **What:** plays and last played, favourite tracks, albums and artists, ratings (tracks), and when albums were added.
  Each can be ticked off.
- **Sources:** three optional reads on the `Source` trait (decision 0005), "this server cannot" by default:
  `track_data` (a page of one account's tracks with plays, last played, favourite and rating; every track played or
  favoured is listed at least once), `favorite_groups` (the account's favourite albums, by one of their tracks or their
  folder, and artists, by name) and `albums_added` (a page of albums with their added date); `imports` says a source has
  them. Jellyfin lists the played tracks, then the favourites (it cannot list "either"), with the account's user data,
  and albums with `DateCreated`; Subsonic lists every song (`search3`, an empty query) and keeps the played, starred or
  rated ones, and reads `getStarred2` and `getAlbumList2`. A Subsonic source reads only its own login's data.
- **Finding them:** a track through the catalogue's link (server id to Cratarr track, then `tr-<id>`); an album through
  one of its tracks, else its folder (the album sharing the most trailing folders with the server's path, all of its
  own at least; none on a tie); an artist by name (`ids::artist`). What the library does not have is counted and left.
- **Accounts:** the page chooses the server account (default: the one chosen last time, else the server's "your
  account") and the Cratarr account.
- **Safe to repeat:** per Cratarr account, source, server account and item, the server's values from the last import
  are kept (`imported` in `userdata.db`, following songs to new ids as their annotations do). Each import then:
  - adds to a song's plays the server's count minus the count imported last time, when more (a count reset on the
    server adds nothing, and later plays count from it), and counts them on its album and artists as a play is counted;
    last played is the later one;
  - sets a favourite or a rating only when the server's differs from the one imported last time, so a change made in
    Cratarr stays until the server's changes. Nothing imported yet reads as "no favourite, no rating": a first import
    brings favourites and ratings in and takes none away;
  - takes away a favourite an earlier import brought in once the server's complete listing no longer has it, unless
    that would take away more than 20 and more than a tenth of them at once (a server that lost track): then they stay
    and the page says so;
  - keeps the earlier added date (the `added` table, as before).

  The max-merge `UserData::import` this replaces is gone; the annotation table keeps Navidrome's shape.
- **Running:** one import at a time, in the background, one page after the other with a 2 s pause between pages (a
  Jellyfin page holds one of Cratarr's four turns, `MAX_IN_FLIGHT`, for its few seconds), a failed page asked again
  after 15 s and 60 s. Preview runs the same import and changes nothing. `GET /api/subsonic/import/sources`,
  `GET` and `POST /api/subsonic/import`; the Settings page's Subsonic section has, per server that can import, the two
  accounts, the four ticks, Preview, Import, the progress and a line of counts.
- **Measured** on the owner's Jellyfin 10.11 (2026-10-15), account `jellyfin`: 5,204 played and 4,789 favourite tracks,
  356 favourite albums, 118 favourite artists, 2,738 albums, no ratings. A page of 250 tracks with their user data took
  9 to 12 s, and an hour later a page of 200 (what the import asks) 4.4 to 5.8 s; 250 albums 5 to 7 s. A whole import
  reads about 10,000 tracks in some 52 pages and 2,738 albums in 11: about seven minutes with the pauses.
- **Not yet:** an import that reads only what changed since the last one (Jellyfin can list the played tracks newest
  first, not the favourites changed since a moment), so running it every few minutes would read everything each time;
  albums' added dates are kept but not yet served (`newest` still orders by the files' age).
