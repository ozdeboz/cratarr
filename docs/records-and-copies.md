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

- `records (id, born, retired_at, title, artist, year, kind, counting)`: a record's id for life, as `tracks.id` is, owned
  or not. Its facts are on the record, so a retired one keeps what it was and one you do not have has them too.
  `counting` is the reference that counts.
- `tracks` gains `record` and its place in it (`disc`, `number`).
- `files.track` stops being one to one: a track has a file in each copy. A **copy** is one folder's files of a record:
  (record, folder, dir).
- `files` gains `came`, `bits` and `rate`. `came` is when the file first came, so a copy keeps its own date when the track
  it holds is another copy's (`tracks.born` is the track's). `bits` and `rate` are what `copy_for` ranks lossless copies by.

**Served:**

- `albums.id` becomes `al-<record>`, whatever the copies; the prefixes and `base_id` go. Ids hold no `/` or `@`, which the
  page cuts on.
- `copies (record, folder, dir, format, min_kbps, size, added)`: what each copy is.
- `copy_files (folder, rel, dir, song, record, src)`: every served file of every copy, with its own numbers. `songs` holds
  one row per track, so a file of a copy that does not play has none: this is what a build compares, and what every lookup
  by folder and path reads.
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

Once it has a record, a folder's tracks say which it is, while more than half of them hold it. Short of that it is held
against the records as a new folder is, by the rules above. A track keeps its record for life, unless its only file now
sits in another record's album: then it moves there. A track that came back from retirement was matched by its recording
or its name, which says nothing of the album its file landed in, so it does not bring a record back or pull an album in.
That takes a track beside it that never left, or more than half of the record's own tracks.

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
2. Identification by the rules above.
3. `copies` and `copy_files` in the served library, `al-<record>`, and `copy_for`. Copies still hold ids of their own, so a
   record is served with the tracks of the copy that plays.
4. The views and the page: dots, menus, the verdicts' end. Then the take: copies take the record's tracks, and a record is
   served with every copy's.
5. The migration, rehearsed on a copy of the dev data, then on staging with production's.
6. Copy to own music.

The take belongs to identification, but it cannot land with it. While an album's id is a hash of its tags, a track shared
by two copies sits in two albums, and two `songs` rows with one id stop the library building. And `al-<record>` with every
copy's rows doubles a record whose copies still hold their own ids. So the ids change first, showing one copy's tracks,
and the take then turns that into all of them by track id. It waits for every reader and writer that took a track to be
one file (which file plays, Delete, a fix reaching every copy, a line on any copy, what a crate counts), and for the merge
of what plays and counts on a track, which it shares with the migration. It stays one small commit, so it can be taken
back alone.

## The rest of 0100

All of 0100 goes in preview 1 (the owner, 2026-11-05). What is above is its owned half; this is the rest, on the same
`records` table: a record with no tracks is a record you do not have.

### What is kept

In the catalogue, beside `records`, growing with what is looked at and never the whole catalogues:

- `references (subject, source, kind, value, at)`: a record's, artist's or label's id at a source. For a record: a
  MusicBrainz release group and release, a Discogs master and release, a Bandcamp album. Unique per source, kind and
  value, so a link finds its subject.
- `reference_tracks (record, source, tracks)`: the tracklist a reference gives, titles and lengths. What judges a folder.
- `artists (id, born, name, sort)` and `labels (id, born, name)`, with `record_artists (record, artist)` and
  `record_labels (record, label, catno)`. An artist's id is for life, like a record's, and served as `ar-<id>`. Two
  artists of one name are one until their references say otherwise, and then they split.
- 0.14's `record_references` and the kept `discography` move into these, and go.

### Merging (0100 point 4)

One pure function, `same_release(a, b)`, beside `same_record`:

1. A source's link to the other: MusicBrainz's Discogs and Bandcamp URLs. It decides.
2. Else the artist, the title and the tracklist, by `same_tracks`.
3. Else two. A name alone never merges.

A discography lists no tracklists, and reading each one costs a request. So a listing merges by links and otherwise stays
apart, and a tracklist is read when a record is opened or asked for, which merges it then. A list can show a record twice
until then; 0100 prefers that to a wrong merge.

### The reference that counts

`counting(references, files)`, pure, the owner's rule: the source whose tracklist agrees with the files; with no files,
MusicBrainz, then Discogs, then Bandcamp. A hand choice (0.14) comes first.

### One missing list

`discover::missing_albums` reads MusicBrainz alone. It becomes three reads into one list:

- **MusicBrainz:** release groups, as today.
- **Discogs:** the artist's masters. New: `discogs::artist_masters`. Albums and EPs only, as the owner chose, and a master
  gives no type in that listing, so the type comes from its main release's format when that is read.
- **Bandcamp:** the artist's music page. New in `bandcamp.rs`.

An artist known to none of the three has no list, and shows only what you own.

### Pages

No new patterns: the pages there are, fed by the record and the artist.

- **A record you do not have** opens the album page: cover, facts, the counting reference's tracks, not playable, Want.
- **An artist you have nothing of** opens the artist page, as Discover's does today, for any artist: profile, related,
  the missing list. `discoverArtist` becomes that one page.
- **A label** opens a page of its own, built of the artist page's parts: its whole catalogue from every source by year,
  what you own marked, the rest one tap from Want. The label on an album's facts opens it.
- **Search's "beyond"** gives records and artists of Cratarr's own, kept on the first look, and opens these pages. Each
  source's site is one tap from the record's references.

### Requests and Soulseek (0100 point 5)

- `request.record` replaces `album_key`, `release_group`, `album` and `artist`, which the record holds. A record any
  source knows can be asked for, so Want works on all of them.
- A fetcher reads the reference it needs: Lidarr takes a release group and says so when there is none, as it does today.
- Soulseek's `Record` points at a record. `soulseek.rs` stops reading MusicBrainz itself and asks for the counting
  reference's tracklist, so a Discogs or Bandcamp record is judged the same way. The one left out: a record with no
  tracklist anywhere is not fetched, and says why.

### The migration, again

The same transaction also: gives each request a record, owned or new; gives each artist an id and rewrites what names
the name's hash (favourites, exclusions, `artist_person`, related artists, the discography's keys); and moves
`record_references` and `discography` across. Each is in the report, and the map keeps old artist ids too.

### Order of work, after step 6

7. `references`, with `counting`, 0.14's record references moving in.
8. `artists`, `labels` and `same_release`.
9. Requests on the record, Soulseek by reference.
10. One missing list: Discogs' and Bandcamp's reads.
11. The pages and search's "beyond".
