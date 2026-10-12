# 0008: A local catalogue of the music Cratarr can see

- Status: accepted; extra folders superseded by [0009](0009-library-uses.md)
- Date: 2026-10-12

## Context

Cratarr kept no index of its own: every question about tracks went to a source, live or through small caches per
feature. That is fine for what is playing, but not for anything over many tracks. On the reference install, with
Jellyfin 10.11 and about 24,000 tracks, a library-wide track query took about 10 s, and reading the tracks of the
5,041-entry playlist through Jellyfin had not finished after 100 s. Playing or exporting a playlist, export sizes,
dead lines, moved files and duplicate albums all need fast answers for many tracks. Each feature also carried its
own cache with its own rules for staleness.

Measured on the same install: walking the music folder (24,187 audio files, 269 GB) took 2.2 s cold and 0.75 s with
size and modification time once cached; the incoming folder (1,170 files) 0.15 s.

## Decision

- Cratarr keeps a catalogue: one SQLite file in its data folder (in memory, rebuilt at start, when there is no data
  folder).
- It covers the playlist folder plus a list of extra folders set in Settings (each one a container mount, read
  only is enough). Cratarr finds which source library lives in which folder by itself, as for the playlist folder
  (decision 0005), so the incoming library needs only its folder listed.
- From the disk, for every audio file: folder, relative path, size, modification time, and the tags Cratarr reads
  itself (artist, title, album, album artist, track and disc number, length, format, bitrate). It does not depend
  on any server, so export names, sizes, moved-file detection and duplicate albums work with any source.
- Per source, in the background: the source's own id for each file, through its paths and the placement root.
  Tracks are paged, then only what changed since the last pass is read. The plugin contract gains a paged track
  listing. A slow or absent source never blocks the page: features that need its ids say "not yet known" for the
  missing tracks.
- Changes are found by a periodic walk comparing size and modification time; Cratarr's own edits update the
  catalogue at once. Tags are read only for new or changed files, a few at a time, so a first fill of a large
  library runs in the background without hurting playback from the same disks.
- The per-feature caches it covers (track items, album summaries, sampling for placement) move onto it as their
  features are touched; nothing is rewritten only for that.
- Interface: the catalogue has no page of its own. Settings shows one status line (track count, up to date or
  progress of the first fill) and the list of extra folders with one sentence of explanation. Features simply get
  faster.

## Consequences

- Two new dependencies: SQLite (bundled, through `rusqlite`) and a tag reader (`lofty`, pure Rust).
- The first fill reads the start of every file once; its duration on the reference install is to be measured
  before release (an estimate goes in the release notes, not here).
- The database is derived data: deleting it only costs a refill.
- Playlists stay the source of truth (decision 0002); the catalogue never changes a playlist on its own.

## As built (2026-10-12)

- Measured on the reference install (24,187 files): the walk takes seconds; tags are read at about 2,100 files a
  minute (about 12 minutes for a first fill) with about 37 MiB of memory and a few percent CPU. The database was
  96 MB after the first fill.
- Tag values are split on read, never in the files. The rules are one set for now (Settings → Libraries → Tag
  splitting): artist fields on "/" and ";" (not "&", "," or "feat.", which are credits), genres also on ",", with
  never-split lists, ID3v1 genre codes as names and case-insensitive repeats dropped. The settings file can carry
  rules per library later; nothing needs one yet.
- Files with problems are classified rather than skipped: *broken* (not readable as audio), *damaged tag* (one
  field unreadable and left out, the rest read), *no tags*, *no length*. They are listed on Repairs. lofty 0.25
  rejects a whole MP3 for some malformed frames in every parsing mode (seen: TIPL with an invalid UTF-16
  byte-order mark, 175 files); a small tolerant ID3v2 text reader then reads the other fields. Files that failed
  with an older reader are read again when the reader improves.
- Each server's ids are linked by a paged listing (500 per page, about 5 s each on Jellyfin 10.11 under load): in
  full once a day, otherwise only tracks the server saved since the last pass.
