# 0101: 0.16 — each user's own music, one track id, and the library on the device

- Status: accepted (the owner, 2026-11-03, by question sets in 632's session: nine questions one by one, then four
  groups of what they had left out; the last three groups on 632's recommendations, the owner: "Yes but go with your
  recommendations"); for 0.16.
- Date: 2026-11-03
- Builds on: 0051 (several users), 0070 (requests), 0084 (each user's own music), 0085 (offline first), 0091
  (scrobbling from the device), 0093 (test and development data), 0096 (libraries equal), 0098 (modes per server), 0100
  (Cratarr's own records)
- Changes: 0084 (where a user's request lands, uploads land in an Incoming, who may Keep, a user who leaves), 0085's
  step 5 (Radio's picker joins the rules crate)
- Open: where 632's recommendations meet earlier decisions, settled later (the owner: "Defer conflicts for later"; the
  last section)

## Context

0.16 is three things in one version (docs/plans.md): each user's own music (0084), the whole library's index on the
device, and crates and the queue on the device (0085's steps 4 and 5). A read-only survey of the code came first
(2026-11-03):
- the libraries' split still rests on `Roles { music, incoming }` (server/src/settings.rs) and "catalogue folder 0 is
  Music";
- served ids are tag hashes, with a folder prefix outside folder 0, and every user-data table is keyed by them;
- signing out clears only the cookies, so downloads, queues, the shelf and the data cache pass to the next account;
- of the crates, track facts can run on a device, while taste, sounds-like and the moment need the server.

## Decision

### Libraries

1. **No default flag: the libraries' list order is the default.** The first main library takes every fallback: the
   first run, a Keep with no target set, Subsonic's order, new users. At the first run every library found becomes a
   main library.
   - Deleting asks twice for a main library's albums, once for Incoming's.
   - Each library has its own colour, shown as a dot only while the viewer has two or more libraries on.
   - Subsonic's `getMusicFolders` lists the libraries by their own names, in the list's order, only those switched on
     for that user.

### Requests and own music

2. **A user's request, once approved, lands in its fetcher's Incoming, like everything; its Keep defaults to the
   requester's own music.** A user may Keep, reject or delete in their own music, and their own requests in Incoming
   (into their own music only); every other library stays the administrators'.
   - Move to… between main libraries renames by the target's template and applies the target's quality limit, as
     Keep does: one plan, one Undo.
   - Keep (`/api/library/albums/{album}/promote`, serve/review.rs) takes a target library. Today it has none. The
     default is Incoming's setting, or the requester's own music for a user's request. Replace works into any main
     library alike, with the target's naming and limit.
3. **Already stored.** The duplicate and better-copy checks run against every library, switched on or not. A better
   copy is fetched as "another copy" of the same record (0100; below). What a request for an album already stored
   does is open (below).
4. **Room.** An allocation per user and a floor for the disk, with Health saying when the floor is reached, and a Keep
   or an upload over the allocation refused with the reason. Their defaults and whom the floor stops are open (below).
5. **Uploads are a fetcher of their own** (changes 0084: not straight into one's own music). They land in an Incoming
   of their own beside Lidarr's and Soulseek's, and their Keep defaults to the uploader's own music. They are checked
   and matched, then given the target's naming and limit at Keep. Their size is open (below).
6. **No private own music.** As 0084 has it: everyone may switch anyone's own music on, and others' start off.
7. **A user who leaves** (Remove, 0051). Their own music stays as an ownerless library that counts against nobody;
   Restore gives it back.
   - While they are away, it is no Keep target, and stays readable to whoever has it on.
   - The account's page offers "Move its music into…" (Move's plan, one Undo) and delete, which asks twice like a main
     library's.
8. **What exists today.** Music becomes the household's main library, owned by nobody. Everyone's own music starts
   empty. Playlists, plays and favourites stay with their accounts. An open request keeps its requester as its Keep
   default.
9. **Own music's folder:** one administrator's setting, with a subfolder per account made at its first use. On a NAS it
   sits beside Music, mounted into the container. Whether it joins the off-site backup is open (below).
10. **Lidarr.**
    - Health says when Lidarr's root folder isn't the folder of Lidarr's Incoming. The root folder is Connections'
      choice, else its artists'. The fix sets Connections' root folder to Incoming's one, where Lidarr has it.
    - Lidarr's album is unmonitored only on a delete in Lidarr's Incoming. Keep and Move already end its watch (0096),
      and a delete in a main library leaves Lidarr alone.

### One track id

11. **A track's key is the catalogue's own track id** (`tracks.id`). It survives Keep, Move, renames and tag fixes, and
    Replace hands the old id to the new file. A path only says where a file is now.
    - Served ids switch to the stable id once, in 0.16. Other Subsonic apps see a fresh library once.
    - A record's id is 0100's record, with its copies apart. A record in two libraries is one record with several
      copies: the best one plays, and an Incoming upgrade is another copy. The folder prefixes go.
    - A gone track's playlist or crate entry is kept as missing, with its title, artist, album and MusicBrainz id. It
      relinks by itself when a match appears. A library switched off only hides it.
12. **Playlists move into the database with new ids,** through the same map as the tracks. `.m3u` files are written out
    after each change, into a folder of their own, and never read back: a readable copy beside the database's backup.

### Accounts on one device

13. **Downloads: a file stored once, with a holder per account.** An account sees only what it holds, and a file goes
    with its last holder.
    - At sign-out, that account's downloads and synced items are hidden but kept, and show again at its next sign-in.
      The sign-out sheet has "Remove this account's downloads".
    - Unsent outbox items, plays and scrobbles are kept under the account, and sent at its next sign-in on the device;
      they go first if the device is online at sign-out.
    - Space: one limit for the device (a shared file counts once). This device shows each account's share, with Remove.
    - When the server says an account was removed, the device lets its downloads go as that Remove would.
    - Hidden is not protected: the files are not encrypted, and the device's own lock guards them.
14. **Cached copies are kept per account and hidden at sign-out:** Cratarr's data, covers, the shelf's covers.
    - Queues are kept per account and mode, Local's too.
    - The person's preferences move to their account's settings on the server and follow them; offline they come
      from the cached copy. These are pinned items, libraries switched off, filters, Home's header picture, crate
      drafts and Radio's last choice. A server mode's Home rows stay on the device, per account.
    - Per preference, the latest change wins, the whole value. The first device that signs in to 0.16 while the server
      holds none sends its own once; every other device takes the server's.
    - The device's own settings stay the device's: zoom, theme, visuals, player name, outputs, speaker delay.
    - Signed out, the app shows sign-in, Local still plays, and nothing of any account shows.
15. **Other servers' sign-ins (0098's modes) are per Cratarr account,** sealed under it, hidden and kept at sign-out.
    - The app works without a Cratarr account: modes added while signed out belong to the device's no-account side,
      as Local does, so the app is a Jellyfin or Subsonic player on its own.
    - Scrobbling from the device (0091) signs in per account, and each play goes out for the account that played it.
      A play with nobody signed in is counted on the device, and scrobbled only by the no-account side's own services.
    - A Play On play is the sender's account's, and Home Assistant's are its key's account's. Plays made by hand on a
      shared player count to the account signed in there; guests have no account. Who scrobbles a Play On play for an
      account its player does not hold is open (below).

### Offline

16. **Sounds like, offline: the whole library's vectors ride with the device's index**, 8-bit (about 4 MB for 21,000
    tracks). The device works out the true list; downloaded tracks play, the others show as not here.
    - The index carries every library the account can see, switched-off ones hidden, so switching one on works
      offline. Small covers for every record ride with it, and full ones for what is downloaded (0085).
    - It syncs whole once (on Wi-Fi, or on mobile data after asking), then only its changes, on any connection. This
      device shows its date.
    - Search offline runs over the index, the library's tier only (0076): the other tiers are left out, with no
      notice.
    - The page asks the browser for persistent storage at its first download. If the browser refuses, This device's
      storage line says so.
    - Other servers' modes, offline: their downloads only, with the crates' track facts; taste and sounds-like are
      left out.
17. **Online the server works crates out; the device does so only offline.** Both run one shared engine, the rules
    crate, so they agree on the same data.
18. **Taste crates offline: the server's fit per track, per account, synced with the index.** These are Fits your taste,
    Second chance and Best match.
    - Counts offline: the server's last numbers, plus this device's unsent plays.
    - Home's New for you and Grave digging show their last copy.
    - **The moment's weather** (the owner's own option): with the server reachable, the server's Home Assistant
      weather. With the server unreachable but the internet there, Open-Meteo: free, no key, at most every 30
      minutes, for where the device is (a coarse location, never sent to Cratarr's server). With no internet, time
      and day only, and the moment says so. A switch in This device, "Weather for the moment crate", off by default.
19. **Radio and Autoplay offline: the server's picker (`radio.rs`, choose and by_mode) moves into the rules crate**
    (changes 0085's step 5). The device runs it over its index, vectors and fits, among what plays here, honouring the
    same settings; the page runs it too.
    - Cratarr mode's queue is saved on the device as well, per account, and Play On takes it up when the server is
      reachable.
    - Offline, Radio leaves out suggestions that cannot play here (Bandcamp and the like), with no notice.
20. **The desktop app (0078) and an installed page follow the page's rules,** each its own device with its own limit.
    The desktop app ships when 0078 does, not as part of 0.16.

### The migration

21. **One step at 0.16's first start.** Every user-data table is rewritten to the stable ids in one transaction. The
    old-to-new map is kept, and a report gives rows per table before and after, and the rows that matched nothing.
    Unmatched rows are kept as missing entries with their remembered tags, never dropped.
22. **Outside references.** Before the release, a read-only list of Home Assistant's references to Cratarr ids; the
    release run rewrites them through the map. Old shared links open through the kept map. Kodi and other apps rescan
    once.
23. **Devices start fresh once** (the owner: "I don't mind reinitializing devices"). 0.16's app clears its data and
    fetches its downloads again: no device-side re-link to build or rehearse.
24. **Rehearsed first.**
    - On staging with a fresh copy of production's data, the report compared with the copy.
    - Then a device reset and set up against staging.
    - Only then production. Rollback is the data copy back, the 0.15 app, and the devices reset.
25. **Backups.** The nightly backup already holds the catalogue, the journal, playlist sync, user data and sessions; a
    new database file joins that list.

### Building it

26. **Two previews.**
    - The foundation: 0100's records, the stable ids with the rehearsed migration, playlists in the database, equal
      libraries, own music. It runs on staging with a copy of production's data.
    - The devices: the index, each account's device data, crates and Radio offline. Then 0.16.
27. **The rules crate moves first, proved unchanged.**
    - Before the first preview, every built-in and saved crate's tracks and Radio's picks (same seed) are compared
      before and after the move, on production's copy.
    - The page runs it as WebAssembly, the app natively through UniFFI (settled, below).
    - Measured on the slowest device in the second preview: a crate under 0.3 s and a Radio pick under 0.05 s, over
      about 21,000 tracks.
28. **Test data (0093).**
    - The fixture library gains two main libraries; an Incoming holding a user's request and an upload; two accounts
      with own music; a playlist entry gone missing; and a 0.15-shaped database for the migration.
    - The end-to-end suite covers sign-out and sign-in (hidden but kept), and a second account on one device. It also
      covers a cut network (search, crates, Radio), and the migration run on the fixture with its report checked.
29. **Scope:** 0.16 is this record and 0100; anything new goes to 0.17.

## Consequences

- **Server:**
  - libraries without roles;
  - the stable ids and the migration;
  - records and copies (0100);
  - playlists in the database;
  - Keep's target, own music's rights and the allocation;
  - Uploads as a fetcher;
  - the preferences on the account;
  - the index and its change feed, with vectors, fits and small covers;
  - the rules crate with Radio's picker in it.
- **Page and app:**
  - the device's store per account (holders, hidden caches, queues per account and mode);
  - sign-in and sign-out;
  - the modes' sign-ins per account;
  - offline search, crates, Radio and the moment's weather;
  - the libraries' dots and Keep's target;
  - own music's pages.
- **At release, in the NAS's own repository:**
  - own music's folder mounted into the container;
  - the Home Assistant rewrite;
  - the new database file in the nightly backup.
- Other Subsonic apps and Kodi see a fresh library once; Cratarr's own devices start fresh once.

## Open: where the recommendations meet earlier decisions (for later)

The owner, 2026-11-03: "Defer conflicts for later". 632's recommendations for the last three groups were made without
0084, 0085 and 0091 at hand. Where they differ, both sides stay here until the owner settles them, before the part is
built.

- **Approval.** 0084: a user's want goes in by itself while they have room in their allocation, with no approval. This
  record's question 2 spoke of an approved request (0070's approval; 0099's amendment: Soulseek takes only approved
  Requests).
- **A request for an album already stored.**
  - 0084: it is put into the requester's own music as a shared-block copy, with no fetch and no space.
  - The recommendation: it shows "In <library>" in place of Want, and Want stays only for a better copy.
- **Room.**
  - 0084: 100 GB per user by default, shared-block copies free. Below 200 GB free, only users' wants and uploads
    pause, the owner's own fetches go on, and the owner is told through Home Assistant.
  - The recommendation: no allocation by default, and the floor stops all fetching and uploads.
- **Uploads' size.** 0084: resumable pieces, no cap per upload. The recommendation: one album at a time.
- **Own music in the off-site backup.**
  - 0084: not backed up, so uploads that exist nowhere else are lost with the NAS (the owner's choice).
  - The recommendation: own music's folder joins the off-site copy.
- **A Play On play for an account its player does not hold.**
  - 0091: the player it plays on scrobbles, never the device that started it.
  - With sign-ins per account on the device (15), that player may not hold the sender's sign-in. The recommendation:
    the sending device scrobbles it then.
- **How the app runs the rules crate.** *Settled 2026-11-03 (the owner, in claude-db's session): as 0085 has it.*
  - 0085: natively, through UniFFI (the desktop app natively).
  - The recommendation: one WebAssembly build for the page and the app, native only if it misses the budget.
  - Settled: UniFFI, native. The rules crate is built for Android and called from Kotlin, as 0.11's Spike B does
    (server/rules-ffi, RustRules.kt); the page keeps WebAssembly. 27's budget is measured that way.

## Amended 2026-11-03: the tape engine joins 0.16 (0102)

The owner decided 0102 the same day, for 0.16: every wind's sound is made on the device, from the track decoded in
memory, with the engine's arithmetic in the rules crate. So:
- **29's scope** is this record, 0100 and 0102.
- **27's move:** the tape engine's arithmetic joins the rules crate after Radio's picker, proved the same way, on the
  server's tests.
