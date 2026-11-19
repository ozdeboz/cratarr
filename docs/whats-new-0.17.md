# What's new in 0.17

Since 0.16.3. What testing 0.16 found, and the rest of decision
[0100](decisions/0100-cratarrs-own-records-artists-labels.md): records you do not have, and artists and labels with pages
of their own. From [0101](decisions/0101-own-music-one-track-id-library-on-the-device.md) come copies into someone's own
music, each user's room and the disk's floor. Both decisions say what changed in their amendments of 2026-11-18.
Then what testing 0.17's previews found (below). Uploads, and the domain core of decision
[0104](decisions/0104-the-domain-core.md), come in 0.18.

**At the first start** 0.17's step moves what 0.16 knew of each record's references into the catalogue, once (below).
Keep a copy of the data folder from before the upgrade: going back to 0.16.3 means restoring it.

## Records you do not have

- **A record you do not have is a page:** its cover, facts and labels, its tracklist (not playable), who has it, and
  Want. Its Bandcamp preview plays from there where it has one. A record that was in Music says until when.
- **Every album not in the library opens its record:** an artist's missing albums, a label's records, search's
  "beyond", Spotify's. Never by its name alone.
- **One missing list per artist,** from MusicBrainz, Discogs and Bandcamp, read in the background. What is kept shows
  at once.
- **Search's "beyond"** lists each album and artist once, by its reference. Two albums of one name stay two.
- **Records you do not have are forgotten** after six months in which no list showed them and nobody opened them,
  unless something points at them: a request, a choice by hand, a Soulseek item. A link to a forgotten one says it is no
  longer known. The library's artists' lists are read again once a month, so their albums stay.

## Artists and labels

- **One artist page by id, for anyone,** with albums here or none: About, related artists, the photo, the missing list
  and the heart. Opened by a name, it moves to the artist's id.
- **Two artists of one name** each have their own page and heart. "Rone (2)" shows where a list holds both.
- **Related artists and Beyond's artists open their artist page,** not Bandcamp or MusicBrainz. So does Discover's
  "Go to artist".
- **A label has a page:** its records by year, had or not, with their catalogue numbers and requests, from MusicBrainz
  and Discogs. More is read as you scroll. A record's labels in its About open it.

## Copies

- **Save to:** an administrator saves a record into someone's own music as a copy, and the original stays where it
  is. One Undo takes the copy away. On btrfs the copy shares its blocks with the original. The history says "Saved to".
- **Move to** takes a record only into a main library.
- **A Want of a record the house has copies it into your music,** with no download: from a main library first, else
  your own music, else someone else's. Only a whole copy. The request says "Copied into your music".
- **Replace in Music from someone's own music:** an administrator replaces Music's copy with a better one from someone's
  own music. The menu says whose: "Replace in Music from Your music", or "from Bob's music". That copy stays.
- **Incoming's upgrades come from copies:** the badge says "Upgrade", or "In Music", with why. A copy with the same
  tracks as an untagged album in Music is an upgrade of it. A different tracklist is another edition, as the intake card
  says.
- **A move, a copy and a Replace do what their sheet showed.** A tap whose copy changed since is refused.
- **The Undo of a copy** is refused whole when a copied file was changed since, and names it.

## Room and the disk's floor

- **Each user has a room,** 100 GB unless an administrator sets another (Settings › Users). Requests show your own room:
  "40 of 100 GB". Only music that has arrived counts, and a copy that shares its bytes with Music or someone's own music
  is free. Administrators have none.
- **Over room, a Want waits,** saying so, and goes by itself once there is room. Nothing is removed. A delete gives room
  back at once.
- **The disk's floor,** 200 GB unless set (Settings › Libraries › Disk space). Below it, users' new Wants wait and
  Soulseek starts no transfer of theirs; administrators' go on. Health says "Short of space", and so does the health
  answer Home Assistant reads.

## Requests

- **"Request albums" is enough:** a Want goes ahead with no administrator's yes. "Approve requests automatically" is
  gone. 0.16's pending requests go ahead at the upgrade.
- **Each request names its record,** so Want works on any record a source knows. An account has one open request of a
  record.
- **Lidarr goes first, Soulseek five minutes later,** or at once where Lidarr cannot take the album or its download fails.
  An album not out yet waits for its release day. The first arrival stops the other fetcher. A fetcher out of reach for
  over an hour shows on Health.
- **Where a request stands is said one way everywhere:** Waiting, Waits for room, Arrived, Partly there (3 of 10), In
  the library, Delivered, Rejected and the rest. A copy missing tracks reads "Partly there", and the fetchers keep
  looking.
- **A request is delivered** once its album is in a library. It stays in the history.
- **You can take back your own Wants,** waiting ones too, and undo your own Add to and Delete in Incoming.
- **Deleting an arrival** rejects its request. Deleting a copy leaves Lidarr fetching for the others who asked.
- **Own music per account,** on or off in Settings › Libraries. Off, the account has no own music, and only an
  administrator adds its requested albums.

## Soulseek

- **Judged by the record's tracklist from any source:** MusicBrainz, Discogs or Bandcamp. A record with no tracklist
  anywhere is Stuck, and says why.
- **Disc folders (CD1, CD2) are one album.** A file named as another version (Live, Remix, Instrumental) is not the
  track. A fuller folder is offered for an administrator's tick when no exact one is found.
- **Placed whole,** through a hidden folder, with the cover beside it. A placement that fails keeps the checked files and
  tries again.
- **Cratarr's own trouble blames no user:** a full disk or a missing folder holds the record, and Health says when
  Soulseek can't store files or place albums.
- **Set-aside folders are kept 14 days.** A queue that can't be read is set aside, and Health says so until the file is
  dealt with.

## Outside services

- **One guard for every call out:** https only, each purpose its own hosts, nothing at a home address, and answers
  capped in size.
- **Bandcamp has a switch** (Settings › Connections). After its browser check Cratarr stops asking it for a day, and
  Health says until when. Its `/music` pages are no longer read.
- **MusicBrainz is asked one question at a time, server-wide,** and Cratarr backs off when it is busy. Last.fm,
  AcoustID and Discogs are paced too.
- **"Pause background reading"** (Settings › Connections) stops what reads in the background. What a page asks for
  still goes.
- Covers known to be missing are not asked for again.

## Safety

- Only administrators and Home Assistant's key can make a player announce, and only from an http(s) address.
- `cratarr-server --version` prints the version and stops. An unknown argument starts nothing.
- A Want's cover is taken only from Cratarr's own covers and known image hosts.
- A user is told when the conversions their page queued end, with their own Undo.

## Memory

- **The analysis pass no longer reads the visuals' signals** to find its next files. Each look read about 1.5 GB of
  the catalogue, and now reads about 25 MB. This was the thrashing seen on 0.16.2.

## The first start

- **0.17's step runs in parts,** each saying in the log as it begins. It moves 0.16's references and discographies into
  the catalogue, gives each request its record, moves the waveforms in three bands to a table of their own, and credits
  the records you have to their artists and labels.
- **Its report** is in `/api/admin/migration`, part "references". A part that fails stops the start and names itself.
- **A second open request of one record** from one account is closed as "Asked twice", and listed.
- **Approved requests no fetcher spoke of** are handed to Lidarr once, when it can be reached.

## From testing 0.16

- **Away from Cratarr,** a page the device can't answer says so in one line ("Discover needs Cratarr."). Playlists are
  listed and opened from the device, and play what is downloaded.
- **Every loader says what it waits for,** a busy Cratarr included ("Waiting for Cratarr").
- **A heart set away from Cratarr stays shown.**
- **Offline search:** a track found through its record's exact title comes above a near spelling.
- **A crate with a Sounds like opens on Most alike:** closest to that sound first, the taste only among those as alike.
  "Closest first" goes.
- **Making a crate:** typed words wait for Pick (or Enter), and its profile shows only when opened. The closeness
  curve's handle stops where it is let go.
- **A device's first Sounds like and Radio pick** answer at once.
- **Play On finds a playlist by its name again,** whatever its capitals.
- **The desktop app** no longer opens blank for minutes when Cratarr's address is silent: after 3 s it uses its own
  copy of the page.

## From testing 0.17's previews

- **One record, its copies and its editions:** two copies of one album are one record. Editions with another tracklist
  stay records of their own, shown as one card that counts them on artist, search and label pages. An EP and an album of
  one name join only when their types agree. A record's type set by hand wins.
- **No source ranks above another:** a busy source is passed over and asked again later, never taken as an answer.
- **Bandcamp is reached only through MusicBrainz's link** to an artist's or a record's page, never by searching a name.
- **An artist's Discogs list keeps their own records:** their albums and EPs, not every compilation they appear on.
- **Who an artist is:** the one whose id their files carry. Namesakes on Discogs and MusicBrainz stay apart, and nothing
  is merged or moved by a name.
- **A label is shown once,** its catalogue numbers together ("Schema · SCEP 317, SCNP 317").
- **Only a present copy is had:** a copy that went or sits in the trash no longer counts.
- **One cover rule:** a record's cover has one address, which tries each of its pictures in turn.
- **A record you do not have** counts the tracks it lists, shows no media provider, and offers Want in its menu too.
- **About's sources:** one chip per source, opening the album's page there, the edition on the same chip. "Tracklist"
  marks a source whose list is shown.
- **Back keeps your place** in every panel, as pages do.
- **The files line** says what it would write ("Not in its files yet: release type album · Write"), and its sheet
  writes it with one Undo. A record's Health line opens its sheet in place.
- **Menus name the record's type:** View EP, Delete compilation, and, on tracks in lists, the queue and Health, Go to EP.
  Save to and Move to as above; Other versions has an icon of its own.
- **Artist chips:** one per credited artist, each with their photo and opening their own page.
- **Loaders:** the mark stays put while its words come and go, sits in the middle of its room, and in a one-row section
  has its words beside it. Where more of a crate is coming: "Reading more tracks: 120 of 340".
- **Search answers quickly:** the lists of missing albums and crates' tracks are no longer made again inside a search.
- **A page that asks the same thing over and over** is held back before the server, and says so in the console.
- **A label page's header** counts the records you have.
- **Health's intake card:** its first row says what this copy is and where it came from; each track plays once, from its
  middle; a length that differs from MusicBrainz's says so ("MusicBrainz says 10:38"); while it reads, its own loader
  says "Checking its files".
- **A cover that failed** is asked again later. Only a real "no cover" is kept.
- **A record's artist chips** open each artist by their id, so two artists of one name never mix.
- **A crate with a Sounds like** asks for itself once, where it asked again and again.
