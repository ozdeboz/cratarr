# Records and copies: how 0100 and 0101 get built

A design for 0.16's last foundation piece. The rules are decisions 0100 and 0101 (its amendments "a track is the record's
track" and "how a record's copies behave"); this says how they are built, and in what order.

## Today

- The catalogue knows folders, files and tracks. A file points at one track, a track has one file in use.
- The served library (`library.sqlite`) is rebuilt from the catalogue. An album is a hash of its tags, with a folder
  prefix outside the first folder (`albums.base_id` is the hash without it), and it lies in one folder and one directory.
- The same album in two libraries is two albums, two sets of tracks. Incoming's verdicts pair them by directory name
  and tags: `upgrade` or `duplicate`.
- User data names albums by those ids in a dozen tables (added, reviews, tags, overrides, references, marks and more).

## The model

**In the catalogue, the truth:**

- `records (id, born, retired_at, was_title, was_artist, was_year)`: a record's id for life, as `tracks.id` is. A retired
  record keeps what it was, as a retired track does.
- `tracks` gains `record` and its place in it (`disc`, `number`).
- `files.track` stops being one to one: a track has a file in each copy. A **copy** is one folder's files of a record:
  (record, folder, dir).

**Served:**

- `albums.id` becomes `al-<record>`, whatever the copies; the prefixes and `base_id` go. Ids hold no `/` or `@`, which the
  page cuts on.
- `copies (record, folder, dir, format, min_kbps, size, added)`: what each copy is.
- `albums` keeps the facts of the record, not of a copy: its folder and directory columns move to `copies`.

## The same record

As the owner decided, the same record is the same tracks. A folder is identified once, when its files are read:

1. The tracklists must match, always: as many tracks, each one's length within 2 seconds, its title the same by the
   loose key. A folder missing a track of the release is not a copy of it.
2. With that, the same MusicBrainz release, where tags or references say one, is the same record; two different
   releases are two records.
3. Without a release on either side, the same album artist and title is the same record.
4. Else it is a record of its own. What cannot be told apart stays two (0100 point 4), and a deluxe edition with bonus
   tracks is its own record by this rule.

A copy takes the record's track ids (0101's amendment), track by track through the same match.

## Which copy plays

One pure function, `copy_for(copies, viewer, libraries_off)`, tested on its own and the only place that decides:

1. A copy picked by hand in the record's menu. The pick is remembered for the account (`copy_picks`: account, record,
   library) until they pick another or that copy goes.
2. The viewer's own music, even at a lower quality.
3. The best quality among the kept libraries the viewer has on: lossless before lossy, then bit depth and rate, then
   bitrate.
4. Nothing else: an Incoming copy plays only when picked, and a copy in a library switched off never plays.

A stream resolves its file through it. `songs` holds one row per track, on the copy that plays for most people, and the
stream asks `copy_for` with who is asking.

## On the page

- A record is listed once. `Album.library` becomes `copies: [{ library, format, kbps, deletable }]`. Dots: one per copy,
  the rule of two or more libraries on standing. A record shows when any of its copies is in a library switched on.
- An Incoming copy of a record already kept shows only on Incoming's list until it is added: no dot on the record's card.
- Its menu names the copy where it has several: "Delete from Music", "Delete from Your music", and "Plays from …" to pick.
  The record stays while one copy is left.
- The quality on a card is the copy that plays.
- A record's Move, Add to and Delete take the copy: `from`, a library's id.
- Incoming's verdicts go: a better copy is a copy of the record, and Replace swaps two copies of one record.

## Copy to own music

`Copy to` makes a shared-block copy (the FICLONE ioctl on btrfs), its files taking the record's track ids. It is free
against the allocation. A file written afterwards takes room of its own (0101: tags and covers go into every copy).
`KeepPreview.copy` says so to the page, and Undo takes the copy away.

On a disk without shared blocks (ext4) it is a real copy, and it counts against the user's allocation.

The off-site backup leaves out shared-block copies of records in Music's mirror, and Cratarr keeps their list for it. A
copy whose original leaves Music is its own from then on, and backed up (0101).

## The migration (0101 point 21)

One transaction in each database, its report the `records` part:

1. Give every album a record, grouping those the rules above call the same.
2. Where two copies hold different track ids for one track, the main library's survives. Plays, favourites, playlist
   entries and every other reference move to it, through a map kept for the report.
3. Rewrite every user-data table that names an album, through `old id → record`.
4. Report each table's rows before and after, and the rows that matched nothing. Those stay as missing entries with
   their remembered tags, never dropped.

The old-to-new map is kept: old shared links open through it, and the release run rewrites Home Assistant's references
through it (point 22). Devices start fresh (point 23). Other Subsonic apps and Kodi see new album ids once.

## Order of work

Each step leaves the server working and its tests green:

1. `records` in the catalogue, `tracks.record`, several files per track, nothing shown differently yet.
2. Identification by the rules above, copies taking the record's tracks.
3. `copies` in the served library, `al-<record>`, and `copy_for`.
4. The views and the page: dots, menus, the verdicts' end.
5. The migration, rehearsed on a copy of the dev data, then on staging with production's.
6. Copy to own music.

## The rest of 0100

All of 0100 goes in preview 1 (the owner, 2026-11-05), so this design is its first half. Still to design, on the same
`records` table, where a record with no tracks is a record one does not have: its references, artists and labels of
Cratarr's own, their pages, the one missing list, search's "beyond", and Soulseek without MusicBrainz.

