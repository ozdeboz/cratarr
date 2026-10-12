# 0011: Feature modules around a functional core; ports where things vary; events between features

- Status: proposed
- Date: 2026-10-12

## Context

So far Cratarr was described as "Rust and Svelte", not by its inner structure. As it stands on main (3158982):

- `api.rs` (HTTP) calls `App` (`app.rs`, 1,437 lines, 33 public methods). `App` holds now playing, transport and
  play, playlist edits, Incoming (lists, verdicts, delete, undo), settings, catalogue access and several caches.
- One real port exists: the `Source` trait (decision 0005), with Jellyfin as its adapter and a fake in the tests.
  The `.m3u` store (`playlists.rs`), the catalogue (SQLite), Lidarr, the trash and file moves are concrete code
  called directly.
- Several decisions are already pure functions over plain data, and those are the best-tested parts: placement,
  choosing what plays, progress, Incoming verdicts.

What comes next does not fit that shape:

- Jellyfin playlists of the user's own, kept in step with the `.m3u` files in both directions.
- Incoming tracks in playlists, which then survive adoption (the files move into Music) and leave on rejection, in
  both copies of every playlist.
- Replace (an upgrade swaps an album's files) carrying playlist entries over.

Two things break. Features would reach into each other through `App`, which already does too much. And a playlist
entry is a file path today. A move changes the path, and Jellyfin gives a moved file a new id (observed before: it
lost play data on a path change), so neither a path nor a Jellyfin id can be an entry's identity.

Hexagonal architecture was the starting point for this decision; what follows takes its lessons, not its ceremony.

## Decision

1. **A modular monolith, cut by feature.** One process, one binary, as now. Inside it, one Rust module per feature,
   each with a service that owns its state and rules:
   - `playing`: what plays, transport, Play;
   - `playlists`: playlists and their copies;
   - `incoming`: lists, verdicts, delete, adopt, replace;
   - `library`: the catalogue, track identity, placement;
   - `settings`.

   `App` shrinks to wiring: it builds the services and the HTTP handlers call them.
2. **A functional core.** Decisions stay, or become, pure functions over plain data: placement, choose, progress,
   verdicts, the playlist merge, what adoption does to playlists. Services do the reading and writing around them.
   Tests aim at the core.
3. **Ports only where something varies or a test needs a fake.** Each is a trait with one real adapter:
   - *media server*: today's `Source`, gaining optional native-playlist operations;
   - *playlist files*: the `.m3u` store;
   - *acquisition*: Lidarr, optional;
   - *files*: moves and the trash, so adopt, replace and delete can be tested without a disk.

   The catalogue's SQLite and the HTTP layer are not wrapped in traits. The driving side is the HTTP API and the
   background pass, both calling services.
4. **Track identity.** The catalogue gives every audio file a Cratarr track id that survives moves and renames. A
   moved file is recognised as the same track by size, length and tags, and by MusicBrainz recording id where
   present. Playlist entries refer to track ids. A path, for an `.m3u` line, and a Jellyfin id, for a Jellyfin
   playlist, are looked up from the track id when a copy is written. A line that points at no known track is kept
   as it is (a missing track).
5. **Playlists belong to the playlist module.** A playlist is an ordered list of track ids. The `.m3u` file and the
   user's Jellyfin playlist are two copies of it, kept in step both ways by a three-way merge against the last state
   both agreed on. Entries may be Incoming tracks. The details get their own decision.
6. **Events between features, in process.** A feature announces what happened; others react. A first set:
   - `TrackMoved { track, from, to }`;
   - `AlbumAdopted { tracks }` and `AlbumRejected { tracks }`;
   - `AlbumReplaced { old, new }`;
   - `PlaylistChanged { playlist, from }`, where `from` is Cratarr or the server.

   For example: Incoming adopts an album and announces `AlbumAdopted`. The playlist module rewrites the affected
   `.m3u` lines (the paths changed) and the Jellyfin entries (new item ids). Rejection removes the entries, and its
   Undo puts them back. Events travel on an in-process channel. Handlers are idempotent, and the background pass is
   the safety net when one is missed.

## Consequences

- The first step is a refactor with no change in behaviour: `App` is split into the feature services one at a time,
  with the tests green after each step. New features wait for it.
- The catalogue stops being disposable. Track ids live there and playlists refer to them, so deleting
  `catalogue.sqlite` would lose them. It becomes part of the data folder that must be kept, which amends decision
  0008.
- Decision 0002 (".m3u files are the truth") changes. The playlist module holds the truth, and the `.m3u` files are
  a kept-in-step copy that stays portable. The playlist decision says so explicitly.
- More moving parts: events need logging so that "why did this playlist change" can be answered, and handlers must
  be safe to run twice.
- Not chosen: separate services or processes, a message broker, CQRS or event sourcing, plugins loaded at run time,
  traits around SQLite. Each costs more than a single-user application on one NAS gains from it.

## Order of work

1. Split `App` into the feature services. No behaviour changes.
2. Add the *files* and *acquisition* ports. Delete and undo move onto them.
3. Track identity in the catalogue, including move detection.
4. Events, starting with adoption and rejection.
5. Jellyfin playlists of the user's own, synced both ways (its own decision).
