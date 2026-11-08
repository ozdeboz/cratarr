# What's new in 0.16

Since 0.15.0. 0.16 is two previews in one release: the foundation (records and their copies, one track id with the
migration that moves everything to it, equal libraries, each user's own music) and the devices (the library kept on
each device, and crates, Sounds like and Radio with Cratarr away). The decisions are
[0100](decisions/0100-cratarrs-own-records-artists-labels.md) and
[0101](decisions/0101-own-music-one-track-id-library-on-the-device.md). The tape engine on the device
([0102](decisions/0102-the-tape-engine-on-the-device.md)) is not part of 0.16: 0.16 plays as 0.15 does, and the engine
comes once it has passed its gate, as the default where it runs, with a switch back per device.

**At the first start** every user-data table moves to the new ids, in one step (below). **Every device starts fresh
once:** it fetches its library again, and each account's downloads come again under the new ids at that account's
own sign-in, on the network its downloads may use. The queue stays.

## Records and their copies

- **A record is one release, whatever holds it.** The same album in Music, in Incoming and in someone's own music is
  one card, with a dot for each copy and what the viewer may do with each. Its tracks are one track each, however many
  files hold them.
- **The best copy plays,** and a record's menu can **Play from** another copy, for that queue only. A far seek, Next
  and Repeat one keep it; pressing Play again is the best copy.
- **What acts on a record names the copy it acts on:** Delete, Add to, Move, Replace, a conversion, a cover or a length
  fix, and each says which library. A tag or a cover written to a record goes into every copy.
- **Artists have ids for life.**

## One track id

- **The catalogue's track ids are the truth.** Plays, hearts, playlists, ratings and every other thing kept about a
  track follow it through a move, a retag or a better copy. A lost catalogue stops Cratarr and says so, rather than
  starting again under new ids.
- **The move to them happens once, at the first start,** with a report for an administrator
  (`/api/admin/migration`): rows before and after, and what matched nothing. Nothing that matched nothing is dropped:
  a missing playlist entry shows what its track was. Old shared links still open.
- **Health** lists what has no file, and a person can say what a track is now.

## Libraries and own music

- **Libraries are equal:** several main libraries, Incoming libraries, and each user's own music, in one list in
  Settings, by kind, in order. Keep, Move and Replace go into any main library; an Incoming record or one in someone's
  own music can be added to a library.
- **Each user's own music:** a library per account, named for who is looking, theirs to add to and delete from. The
  Users panel handles someone who leaves.
- **Playlists live in the database.**
- **The person's preferences are kept on their account:** pins, libraries switched off, filters, told live to every
  device.

## The library on each device

- **Kept on the device, whole once and then what changed:** the library's index, the sounds behind Sounds like, the
  account's own counts and fits, and every record's small cover. In the app the app keeps it, with no page open. This
  device shows when it last stood where Cratarr does, and asks before a whole fetch on mobile data.
- **With Cratarr out of reach:** browsing shows what is here, what is not, and the covers; search finds the library;
  crates, Sounds like, Radio and Autoplay are worked out on the device, among what plays here.
- **Cratarr gone mid-play:** the player stops at the place reached, keeps the queue, and carries on when Cratarr
  answers. The queue is kept on the device for each account and mode, and Play On takes it up.
- **The moment's weather** from Open-Meteo while Cratarr is away, if switched on in This device (off by default).
- **Several accounts on one device stay apart:** hearts, crates, plays, the library's own counts and downloads are each
  account's, through a sign-out and another's sign-in, and come back as they were at the next.
- **A download whose file is gone** shows as not here, and can be fetched again. A kept list that cannot be read says
  so, and starts again with the music staying, each download given back to whoever held it last.
- **Sounds like of many seeds** is about twice as quick, with the same answer. The app frees the memory it answered
  with as soon as Cratarr is back.
- **Lists and grids ask covers at the size they show.**
- **The app asks once, as it starts,** for what playing in the background needs.

## Fetching

- **A request by name finds the album Lidarr has,** and keeps its id.
- **A Soulseek search gets its full 15 seconds.**

## Crates

- **A crate's favourites toggle never set takes favourites in, everywhere:** on the Crates page, when opened and when
  played. Unlisted and Second chance included, which played without them before.

## Playing

- **Headset and car Next and Previous** go where the page's controls point.
- **The media session hears every event of the player again** (lock screen and car controls).
- **The waveform's bars stand still during a held zoom.**
- **The outbox:** a play Cratarr can't count yet waits rather than being sent straight back; a 403 or a change cut off
  half way no longer stalls it.

## Safety

- **Changes to files:** a run is written down before its first file, a stop keeps to what it wrote, an Undo puts the
  originals back, and a library that reads empty is never taken for every file deleted.
- **Administrators only:** reviewing Incoming, an album's tags, sharing playlists in a batch. An app password signs in
  to no page.

<!-- From the commits since v0.15.0 on 632/own-music at 3a3b948d3 (2026-11-08): D21, P4's Sounds like and favourites,
     the offline review's six findings and the fresh start included. D22's tablet numbers, if they change anything,
     come after. -->

## 0.16.1

- With Cratarr out of reach and another player chosen, music plays on this device. Once it has played here, this device stays the chosen player.
- Covers that failed to load while away load again when Cratarr is back, without a reload.
- A waveform no longer stays flat after the network drops: each analysis request gives up after a minute.
- While the library on this device opens for the first answer away from Cratarr, the page shows its loader and says so.
- Away from Cratarr, the app keeps taking the page's commands: taps in the queue, Play and Next work offline, and an offline start opens in seconds instead of waiting for ever.
- A network that goes silent counts as away within seconds, so the page opens on what the device has.
- Play from a record's menu in Downloads and in search results works again.
- Available offline lists albums synced to the device.
- Space plays and pauses everywhere.
- A device that comes back to a group it led pauses and keeps its queue, and "This device" brings the other screens along.
- Every Move, Add and Replace on a record of several copies names the copy it acts on, and the player's Quality and File describe the copy that plays.
- Search shows its answer in one step, the crate card with the results; artists with tracks only show no album count.
- An album action's page in Changes to files shows the files it moved, and its Undo.
- Health's missing tracks say when nothing was kept of them.
