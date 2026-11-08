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
- `track_moved (old, id, followed)`: a track a record's track took, what it became, and whether the user data has followed.

**Served:**

- `albums.id` becomes `al-<record>`, whatever the copies; the prefixes and `base_id` go. Ids hold no `/` or `@`, which the
  page cuts on.
- `copies (album, folder, dir, format, min_kbps, size, added, tracks, bits, rate)`: what each copy is. `album` is
  `albums.id`, the record as it is served, and `added` the first `came` of its files. A copy is as good as its worst file:
  one file not read leaves its `bits` and `rate` unknown, and it ranks last.
- `copy_files (folder, rel, dir, song, album, src, ...)`: every served file of every copy, with its own numbers. `songs`
  holds one row per track, so a file of a copy that does not play has none: this is what a build compares, and what every
  lookup by folder and path reads.
- `albums` keeps the facts of the record, not of a copy: its folder and directory columns move to `copies`.

## The same record

As the owner decided, the same record is the same tracks. A folder is identified when its files are read, once, or over its
first day where it arrives file by file:

1. The tracklists must match: as many tracks, each one's length within 2 seconds, its title the same by the loose key. A
   guest in a title is recognised: "Embrace (feat. X)" is the title "Embrace" with a guest, so it matches "Embrace", and
   "Embrace (feat. Y)" does not. A folder missing a track of the release is not a copy of it, but for one case: see "A folder
   of the very same release".
2. With that, the same MusicBrainz release, where tags or references say one, is the same record; two different
   releases are two records.
3. Without a release on either side, the same album artist and title is the same record.
4. Else it is a record of its own. What cannot be told apart stays two (0100 point 4), and a deluxe edition with bonus
   tracks is its own record by this rule.

A copy takes the record's track ids (0101's amendment), track by track through the same match: its files go to the record's
tracks, and the ids they held retire for good. A file replaced outside Cratarr in one copy, with an id of its own, takes the
track that copy has no file of, by its name and length. `track_moved` keeps what each id became, for as long as there is
anything that could name it: a taken id never comes back to a file, what names it follows it (plays, favourites, playlist
entries and the rest, at the next build), an Undo from before lands on the record's track, an old id in a queue plays it,
and AudioMuse's export finds its track by it.

Copies of one record always hold the same tracks, which is what the ids say once taken: a retag or a length fix in one
copy splits nothing. A track added to one of two copies does (the owner, 2026-11-06), as a deluxe edition is its own
record. Who keeps the record, its id and what points at it goes by kind first: a main library, then someone's own music, then
Incoming. So Music keeps it from an Incoming copy with a track more, and one account's own copy never takes it from Music.
Between copies of one kind the one with more tracks keeps it, the first on an equal count. The other is held against the rest
as a new folder is, with tracks of its own. That goes ahead of "a track keeps its record for life".
A copy that lost a file for good leaves the same way; a file gone a while or not read splits nothing, nor does a copy an album
action is moving, and the record lists the track from the copy that has it. Among copies of one kind, where one gained more
than it held no majority holds it to the record, so it leaves whatever its count: the record stays with the copy that is still
it. Copies of a lower kind do not count against it there.

Once it has a record, a folder's tracks say which it is, while more than half of them hold it, or nothing else does: a
folder holding every file of a record that is left keeps it, however many tracks came in beside them. Short of that it is
held against the records as a new folder is, by the rules above. A track keeps its record for life, unless its only file now
sits in another record's album: then it moves there. A track that came back from retirement was matched by its recording
or its name, which says nothing of the album its file landed in, so it does not bring a record back or pull an album in.
That takes a track beside it that never left, or more than half of the record's own tracks. A record whose tracks have
all moved on is retired, as one whose tracks retired is: it was had, and it keeps what it was.

A folder arriving file by file joins the record you hold (the owner, 2026-11-06). It is short of that record until its last
file, so it is a record of its own for a while: a record less than a day old, alone in one folder, with no track shared with
anything, is held against the others, and joins where `same_record` says yes. Its tracks are taken and the young record goes.
Older records never merge, so nothing played or favourited moves by this.

### A folder of the very same release

A folder with the exact MusicBrainz release id of a record joins it at once, from its first file, even with tracks still missing
(the owner, 2026-11-07). The tracklist rule bends for this case only; without that very id a folder waits for its last file, as above.

- **A file takes the track it is, and no other.** By name and length, each to one track of the record's, whatever order they come in:
  place by place a folder's second file would take the record's second track, which may be a track it does not hold. Of two tracks
  alike a file cannot say which it is, and a take cannot be undone, so that folder waits for its last file too.
- **A track the record does not list keeps a folder out.** It is short of the record, or it is something else.
- **It works the other way round too:** a whole folder landing beside a few tracks of the release you hold is that record, and what
  it brings are tracks of it. Who keeps the record goes by kind as ever, so Music's short copy keeps it from a whole one in Incoming.
- **Short of what the record has, a copy stays,** arriving or for good: its ids hold it there whatever the tags say since, and a copy
  of the very release that lost a file for good stays short rather than leaving. A track new to the record is another matter: there
  the other copy grew, and the one that holds all the record ever had leaves by the rule above, the same release or not. It does not
  come back as a folder short of it.
- **Nothing old merges:** this is for a folder not yet held by a record. Two records of one release from before stay two, but at the
  first start on 0.15's data, where every album is a new folder.

What a short copy means while it arrives:

- **Play:** nothing changes until it is all there. A whole copy plays before one short of tracks, whatever its quality, so a better
  copy on its way does not turn a record into three tracks of FLAC and seven of something else, its cover and facts with it. Among
  copies all short of something, quality decides, as it does among whole ones. A file gone a while makes its copy short for that
  while, by the same rule.
- **`from`:** it hears the copy for the tracks it holds, and is passed over for the others. An Incoming copy still plays only by
  `from`. A track only an Incoming copy holds, of a kept copy short of it, is that track's only copy, and plays from there.
- **The panel:** a copy's line says "3 of 10" in its size's place, as how much of it is there says more than its bytes.
- **Replace** is refused: it would put the tracks the copy lacks in the trash. **Delete** takes the copy's own files, and the record goes
  on. **Add to** moves what is there, still short.

## Which copy plays

One pure function, `copy_for(copies, viewer, libraries_off)`, tested on its own and the only place that decides:

1. The best quality among the kept libraries the viewer has on: lossless before lossy, then bit depth and rate, then
   bitrate. Their own music counts as any kept library, so a FLAC in Music beats their own MP3, and their own FLAC beats
   Music's MP3. Someone else's own music starts off, and counts only while it is on.
2. Nothing else: an Incoming copy of a kept record never plays by itself, even when it is the better, as nobody has checked
   it. It plays once added, or when it is the only copy. A copy in a library switched off never plays. A record only in
   Incoming has nothing else: its copy plays.

A whole copy comes before one short of tracks, whatever its quality (above). A tie goes by the libraries' order, then the path.
Lossless copies not read since bit depth was kept rank last.

Nothing is remembered. A request can name a copy, `from`, a library's id: while its library is on, that copy is heard, with
an account or without one, for that one request. "Play from" is a one-time action built on it: it starts a new queue of the
record from that copy, each track of the queue carrying `from` (`QueueTrack.from`), and the next Play is the best copy again.

The crates, the words and the taste read a track once: one `Facts`, from its row copy's file, holding the line and the folder of
every file of it. A set kept by line finds the track through any of them.

A stream resolves its file through it. `songs` holds one row per track, on the copy that plays for most people: `copy_for`
with no viewer, where own music counts as off unless nothing else holds the record, and there the first copy added stands. The
stream asks with who is asking;
a copy lacking the track is passed over, and where nothing plays for them the row copy does. With no one in particular
(a signed link while signing in is required) it is the row copy too. What is read of a track, its waveform, beats and
gains, is the file that plays, so a waveform's ETag names the file.

## What a record says

A record's facts are shared, and a person's own copy never changes what others see (the owner, 2026-11-07).

- **A record in a main library goes by it:** the name, artist, year, genres and release are that copy's, the one that plays for
  most people.
- **Where it says nothing, another copy fills in,** as a record must not lack a year or a release for that: another main library's or
  Incoming's. Never someone's own music.
- **A record only in people's own music goes by the first copy added,** whatever a later one's quality, and that copy alone speaks. A
  listener still hears the best they have on: this is what the record says, not what plays.
- **Who may change it:** "If it's owned by administrator, then only administrator can edit." A record in a main library or in Incoming
  is the administrators', a copy of one's own beside it or not. A record only in people's own music is its first copy's owner's, and
  the administrators' as ever. That goes for the facts set by hand. Its cover, tags and references stay the administrators': a cover goes
  into every copy, someone else's files among them.

## Where things stand when a library changes place

A library's folder has an id for life, and its path is a name that can change.

- **A switch follows its library.** What an account has off, and what Radio and Autoplay choose from, say a library by its path. A
  rename or a moved mount is told to the user data, in one breath or through a start that found it parked, and each account's lists
  follow, dated now, so a device's copy from before cannot win the old path back.
- **Every table by folder and path goes with its file:** `FILE_TABLES`, and a test fails on one left out. `handovers` is the one that is
  not a file's: a promise about a place a file is on its way to, which taken along would be gone before the file came.
- **A file's own size, time and length come first** when a gone file is looked for: a `mv` keeps them. The rules after it go by tags and
  names, and ambiguity is never guessed, under any of them.
- **A library out of use is forgotten by nobody's clock.** Its rows wait, however long, and it comes back as itself. Only other files found
  at its path forget it.

## Missing music

Health lists what Cratarr knows of and has no file for, to administrators: libraries out of use, tracks with nothing but files gone a
while, and retired tracks that hold plays, hearts or ratings, each with where it lay and since when. Nothing there is forgotten by itself.

A track there can be reconnected by hand: what no rule could pair, a title and a file name changed at once with no recording id. A
person names the track it is, and it goes as a take goes, through `track_moved`, for good: its plays, hearts and lines follow at once,
and an old id in a queue plays the one chosen. It has no Undo, as a take has none.

## On the page

- A record is listed once. `Album.library` becomes `copies: [{ library, format, kbps, bits, rate, deletable, keepable,
  keep_to, lacks, plays, added, size, tracks, job }]`, what the viewer may do going by the copy. They come in the order they
  would play for that viewer, `copy_for` asked again of what is left, and a copy in a library off keeps its place, so the
  first one on is what plays and a switch thrown on the page needs no new list. The card's format and bitrate are the
  copy that plays. Dots: one for each library foreign to home, the first kept library in the libraries' order, main or
  own music alike, on or off. Home has one only beside another kept library, so a duplicate shows both. Every other kept
  library has its dot, and every Incoming one, beside a kept copy too. A server of one library has none, and a card
  reached with all its libraries off names where it lies. A record shows when any of its copies is in a library switched on, and so
  does a track, in every list and count: `crates::on_for`, the one test. An Incoming copy counts there, though it plays only
  by `from`: with the kept library off, that is where the record is found.
- A playlist's line names a file, and an entry is its track: on any copy it has the track's id and plays, what plays being
  `copy_for`'s to say. A line written now is the row copy's. Where its copy goes, or leaves the record, the entry reads the file
  its track has left, at once, and the minute's pass writes that line.
- An Incoming copy of a record already kept shows only on Incoming's list until it is added, and by its dot on the record's
  card (the owner, 2026-11-06).
- Its menu names the copy where it has several: "Delete from Music", "Delete from Your music", and a "Play from …" for each
  other copy in a library on, Incoming too, shown only where there is one. The record stays while one copy is left.
- The quality on a card is the copy that plays.
- Third-party apps see every library: the Library page's filter is the page's and does not reach Subsonic's music folders (the owner,
  2026-11-07). It stays on the account, and still says which copy plays.
- A record's Move, Add to, Delete and Replace take the copy: `from`, a library's id, in the query for a preview and the body
  for the act. A record of several copies with none named is refused, and nothing moves. Of two folders of one library
  `from` names the first. Replace also takes `to`, the main library whose copy it takes the place of.
- A copy goes whole only where it is alone in its folder: every audio file under its `dir` is its own, and no other copy lies
  inside. Else it reads as not seen, as `dir` is only what its files share, and can be a parent of other records or the root.
- An intake card is about the copy waiting, whichever plays: its tracks are that copy's files, what is heard is kept by
  (record, folder), a fix on the "Before adding" sheet writes to that copy alone, and a snippet's stream names it by `from`.
  A review is kept by (record, folder) too.
- `from` is in the query of `/api/stream`, its wind and rewind, and `/api/file`, which is what the app and Kodi play. A queue's
  track carries it on every player, as it carries the rest: a player asks with it, and Kodi is given addresses that hold it. A
  copy gone or in a library off is passed over, so such a queue goes on from the best copy. Downloads keep the file they saved.
- A stream's file is named in its validator, and `If-Range` is honoured: a seek never gets another file's bytes.
- A tag written by Cratarr is the record's or its track's, so it goes into every copy: Health's fixes, Renumber, a match with
  MusicBrainz, the tempo, the files' facts and words. Reading stays one file a track, and writing takes every file, each saying
  for itself what it holds, so one Undo puts each file back as it was. What links two copies' files is the track, never the
  file name. Only "Before adding" writes to one copy. Health's problems on a record's page are every copy's.
- A cover goes beside and inside every copy too, the picture fetched once. Each copy is one file of the run with its own
  kept pictures, as copies can hold different ones, so Undo gives each its own back. What it goes inside is the copy's files
  as the library has them, not what lies in its folder.
- A length fix stays in its file: copies differ in length, so it goes by the file's place, never by the track.
- On a sheet, what a track's copies change alike is one row. Where they differ, each row has a `library`, as `copies` names it.
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

It runs at the first start, before anything has read a store, before the first build and before the first request
(`app::first_start`). First the catalogue, in one transaction: every album's record, the merge, each id's map (`album_moved`,
`artist_moved`, `track_moved`). Then the journal's changes, the sync's agreed lists and the JSON stores, then the user data, in one
transaction, the `records` part written last in it. No commit spans the databases, so each step is safe to go over again, and that
part alone says it is over: a stop anywhere finishes at the next start, and leaves what a start that never stopped leaves. A failure
stops the start, as Cratarr does not run on half-moved data. The way back is the copy from before (point 24).

- **An old id names one copy**, so one record, and the map keeps its folder: `album_moved` form `id`. Music's form of it, `base`, can
  name two records, as the same tags on other tracks are two records, and what went by it goes to both.
- **Of two tracks found to be one the main library's stays:** albums come in main libraries first, in their order, then own music,
  then Incoming, and what no list shows after all of those, so a file that went decides nothing.
- **What no list shows gets its record too:** files gone a while, in a trash, or in a parked folder, by the id they were served
  under. With none, an Undo or a source switched on again brings back an album that has lost what named it.
- **An album can be named by an id it does not have now.** A Keep left its plays under the id it had in Incoming. A copy kept in Music
  and gone left what went by Music's id while the release waits in Incoming, and that id survives only as the album's `base`, which no
  table of ids walks. A folder's id was its place in a sorted list, so a library added in front moved the next one along, and what came
  before lies under the place it had, beside what came after. So each album keeps the ids it would have had in Music and in each
  Incoming folder, unless a real album holds one, and what lies under two of them folds as any two ids of one record do. Such an id is
  of the copy where it lies now, so a review or a hearing reads on that copy and on no other. Of a release on two records, the copy in
  Incoming comes first. Where a real album holds the id, it stands: that is what 0.15 shows on it, and one row cannot be parted.
- **What stops it:** a library not there, which says which, and a catalogue whose ids were lost, or one lost before 0.16 remembered
  it, as what names its ids would all read as matching nothing. Once it is over, a catalogue from before it beside user data from after
  it stops the start too: restored alone, it would number its albums anew under every row, with no error. A generation cannot tell, as
  a backup has the one it always had, so the catalogue's own mark is asked, and one started afresh sets it.
- **A file's shape and beats follow it** where its library changed place, as 0.15 took `analysis` along and left these at the folder it
  had. Where two libraries hold the path, nothing says which file a row is of: it stays, and the report counts it.
- **What matches nothing stays and is counted,** as a tag's hash from before a retag cannot be read back. A left-out line on a file
  in a trash stays a line too: its path went with it.

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
`records` table: a record with no tracks, not retired, is a record you do not have.

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
