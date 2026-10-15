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
