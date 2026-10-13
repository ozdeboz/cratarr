# 0011: Feature modules around a functional core; ports where things vary; events between features

- Status: accepted
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
   - *media server*: today's `Source`, gaining optional native-playlist operations, and copying a track's play data
     (plays, favourite) from one item to another. Since added, all optional: hearing of moved folders
     (`files_changed`, decision 0017), the state of its library scan (`library_scan`, decision 0017), an album's
     track count, format and bitrate (`album_summary`, asked only where the catalogue cannot tell), a cheap summary
     of what its players do (`players`, for the page's live channel), and playing tracks from a position
     (`play_from`, decision 0017);
   - *playlist files*: the `.m3u` store;
   - *acquisition*: Lidarr, optional. Each call returns the state it replaced (which albums were monitored), so it
     can be undone. It is best effort: an action stands when Lidarr fails;
   - *files*:
     - `trash(folder, path) → token` and `restore(folder, token)`. The trash lives inside the folder itself, so
       trashing is a rename on the same file system, never a copy;
     - moves for adopt and replace;
     - `can_write(folder)`: the write test of decision 0010.

     This way adopt, replace and delete can be tested without a disk.

   The catalogue's SQLite and the HTTP layer are not wrapped in traits. The driving side is the HTTP API and the
   background pass, both calling services.
4. **Track identity.** A Cratarr track id means "this recording in my library", not "this file". The catalogue
   holds it, and playlist entries refer to it.
   - **Writing the copies:** a path (for an `.m3u` line) or a Jellyfin id (for a Jellyfin playlist) is looked up
     from the track id when a copy is written. A line that points at no known track is kept as it is (a missing
     track).
   - **The feature that moves or replaces a file updates the catalogue itself,** in the same step as the file
     operation. That happens before a walk could see "old gone, new appeared" and break the identity. The walk only
     catches changes made outside Cratarr: it recognises a moved file by MusicBrainz recording id, length and a
     matching path, never by size or tags alone. That way identity survives Cratarr's own retagging and format
     changes.
   - **Replace pairs old and new tracks** in the core. The first rule that matches wins:
     1. the MusicBrainz recording id;
     2. disc and track number plus the folded title;
     3. the length, within a tolerance.

     A paired new file inherits the old track id, so playlists need no rewrite. An unpaired new file gets a new
     id. An unpaired old id becomes a missing track and shows up in Repairs.
   - **Trashed tracks keep their ids,** marked as trashed. Rejection takes them out of the playlist copies, and Undo
     marks them back so the entries return; no list of entries is stored for that. Only purging the trash, after
     the set number of days, retires the ids.
5. **Playlists belong to the playlist module.** A playlist is an ordered list of track ids. The `.m3u` file and the
   user's Jellyfin playlist are two copies of it, kept in step both ways by a three-way merge against the last state
   both agreed on. Entries may be Incoming tracks. The details get their own decision.
6. **Events between features, in process.** A feature announces what happened; others react. An album is
   handled as one unit: one event per album action, carrying its tracks, so one Undo reverses it. A first set:
   - `AlbumAdopted { album, moves: [(track, from, to)] }`;
   - `AlbumReplaced { album, moves: [(track, old file, new file)], added, retired }`;
   - `AlbumRejected { album, tracks }`;
   - `TrackMoved { track, from, to }`, for a single file (a rename in Repairs, a fixed name);
   - `PlaylistChanged { playlist, from }`, where `from` is Cratarr or the server.

   Who reacts to an adopted or replaced album:
   - the playlist module rewrites the affected `.m3u` lines (the paths changed) and the Jellyfin entries (new item
     ids);
   - the media server copies play data from each old item to its new one, since Jellyfin gives a moved file a new
     item;
   - acquisition tells Lidarr to leave the album alone, so it does not fetch it again.

   **The journal:** every event is written to a small journal in the data folder before it is handled. An entry
   holds the event id, the time, the event and which handlers are done. The event id makes each handler safe to
   run twice. The journal lets handling resume after a restart, keeps Undo working across restarts (Delete's Undo
   is held in memory today), and answers "why did this playlist change". Events travel on an in-process channel,
   and the background pass picks up anything the journal shows as not done.

## Consequences

- The first step is a refactor with no change in behaviour: `App` is split into the feature services one at a time,
  with the tests green after each step. New features wait for it.
- The catalogue stops being disposable. Track ids live there and playlists refer to them, so deleting
  `catalogue.sqlite` would lose them. It becomes part of the data folder that must be kept, which amends decision
  0008.
- Decision 0002 (".m3u files are the truth") changes. The playlist module holds the truth, and the `.m3u` files are
  a kept-in-step copy that stays portable. The playlist decision says so explicitly.
- More moving parts: the journal must be kept with the data folder, and every handler must be safe to run twice.
- Not chosen: separate services or processes, a message broker, CQRS or event sourcing, plugins loaded at run time,
  traits around SQLite. Each costs more than a single-user application on one NAS gains from it.

## Order of work

1. Split `App` into the feature services. No behaviour changes.
2. Add the *files* and *acquisition* ports. Delete and Undo move onto them.
3. Track identity in the catalogue: inheriting ids on replace, the trashed state, and detecting moves made outside
   Cratarr.
4. The journal and events, starting with Delete (rejection). Keep (adopt) and Replace are then built on them.
5. Jellyfin playlists of the user's own, synced both ways (its own decision).
