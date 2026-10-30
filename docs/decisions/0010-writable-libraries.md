# 0010: One writable mount; Cratarr tests where it can write

- Status: accepted
- Date: 2026-10-12; amended 2026-10-13 (what Cratarr writes, and the permissions of the folders it makes)
- Amends: decision 0009 (the read-only parent with only the Music folder writable inside it)

## Context

Decision 0009 mounts one parent of all music libraries, so a new library never needs a compose edit. Only the Music
folder was written then (playlists and their backups), so the parent was read only with Music mounted writable inside
it. Deleting an album from Incoming writes in the Incoming folder too. A second writable mount per Incoming library
would bring back the compose edit 0009 removed.

The *arr apps share one writable `/data` mount between them and the download client, so moves stay instant. They
test a root folder by writing to it when it is added, and show a folder they cannot write to as a health warning,
in one place. Deleted files can go to a recycling bin that is emptied after a number of days.

## Decision

- Install with one writable mount that holds the music libraries. Cratarr writes only in the music libraries, and
  there only:
  - in the Music folder: playlists, their backups and `.playlists` (decisions 0002, 0014), and the albums Keep and
    Replace move in (decision 0013);
  - in each Incoming folder: `.cratarr-trash` (deleted and replaced albums, converted-away originals) and
    `.cratarr-convert` (conversion's work in progress, decision 0013).

  Other libraries in the same mount (films, series) are left alone by the code, not by the mount.
- A folder Cratarr makes gets the permission bits of the folder it is made in (`moves::create_dirs_like_parent`),
  whatever Cratarr's umask. Media folders shared by a group are typically 2775; Cratarr's own umask (022) made the
  first kept album's folder, `.playlists` and the trash 2755, which the group could not write in (2026-10-13).
- Cratarr tests a folder by writing a hidden file and removing it (`mounts::writable`). A read-only mount and wrong
  ownership both fail it.
- Settings says so on the library: "Cratarr can't write here". In Incoming, Delete is only offered for albums whose
  folder Cratarr can see and write (`deletable`), so it never fails after the question was answered. The test is
  repeated at most every ten minutes.
- The trash stays one hidden folder per Incoming folder (an instant rename; a single bin elsewhere would turn Delete
  into a copy across disks). How long it keeps albums is set in Settings: 14 days unless changed, 1 to 3650.

## Consequences

- A bug that writes outside those places is no longer stopped by the mount. The writes go through `trash.rs` and
  `moves.rs` (plain relative paths only, never `..`, into a hidden folder only for the trash), conversion's staging,
  and the playlist store.
- The reference install mounts `/Volume1/media` at `/media`, writable.
