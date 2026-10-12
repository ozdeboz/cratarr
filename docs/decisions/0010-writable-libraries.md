# 0010: One writable mount; Cratarr tests where it can write

- Status: accepted
- Date: 2026-10-12
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

- Install with one writable mount that holds the music libraries. Cratarr writes only in two places: playlists and
  their backups in the Music folder, and deleted albums into `.cratarr-trash` inside their Incoming folder. Other
  libraries in the same mount (films, series) are left alone by the code, not by the mount.
- Cratarr tests a folder by writing a hidden file and removing it (`mounts::writable`). A read-only mount and wrong
  ownership both fail it.
- Settings says so on the library: "Cratarr can't write here". In Incoming, Delete is only offered for albums whose
  folder Cratarr can see and write (`deletable`), so it never fails after the question was answered. The test is
  repeated at most every ten minutes.
- The trash stays one hidden folder per Incoming folder (an instant rename; a single bin elsewhere would turn Delete
  into a copy across disks). How long it keeps albums is set in Settings: 14 days unless changed, 1 to 3650.

## Consequences

- A bug that writes outside those two places is no longer stopped by the mount. The writes go through
  `trash.rs` (plain relative album folders only, never `..`, never the trash itself) and the playlist store.
- The reference install mounts `/Volume1/media` at `/media`, writable.
