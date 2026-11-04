# Plans

Big ideas that are not decided and not being built. Each entry gives what it is, why it waits, and the catches known so
far. A plan that is taken up becomes a decision in [decisions](decisions) first, and its entry here points to it.

Most of these wait for the same reason. Cratarr is a **thin client**: the server holds the library, the queues (0033)
and the pages, and the app shows the server's pages, with a native player under them (0031). The app keeps tracks for
offline play (0038), but it only knows what the server tells it. Each plan below asks the phone, or a car, to know
something the server doesn't.

## The runway (the owner, 2026-10-30)

What comes after 0.10, in order. Each becomes a decision when it is taken up.

- **0.11: quick crates.** A crate made from a few typed terms: a genre, an artist meaning "sounds like" that artist,
  a label or an album, each a chip that can switch between "like" and "by"; filtered first, then ranked, with a
  closeness cutoff and the artist's own tracks left out. (The Library's Tracks page already went, in 0.10.) With it: a
  mark on the tracks that have other versions, and a spike of the rules crate inside the Android app, behind a switch.
  *On trunk:* quick crates (the closeness curve counted among the crate's other words still to come), labels in
  search, the other-versions mark, the spike (done: one crossing per frame for 0075), the server's capabilities, the
  page served from a folder, and the apps' own pages ([whats-new-0.11](whats-new-0.11.md)).
- **0.12: Classical's works and movements** (a composer, their works, each work's recordings, movements grouped under
  their work), with offline first's steps 2 and 3 whole beside it: Downloads an offline shelf, and nothing done offline
  lost, playlist edits included (the server's change log; below). Modes per server moved to 0.13 (the owner,
  2026-10-30: deferred, the change log pulled forward instead). *On trunk:* all of it, and with it Grab a crate as a
  sentence, snippets, one search matcher, the steady waveform, one table head, menus on search results and works,
  and menus that offer only what works away ([whats-new-0.12](whats-new-0.12.md)); works in global search wait for
  the server's search kind.
- **0.13: what preview.3 holds, frozen** (the owner, 2026-11-01: what remains moves on rather than holding the release;
  out once the owner has tried preview.3 enough). One crate in two views and only in the panel, more words in the
  sentence, the crate card atop every search; a page's search as the table, its empty answer from the magnifier, one
  loading rule; scrolling to what a link leads to, clear of the chrome; the glass moving as one (lib/glassMotion:
  ContentControls, NavigationControls and the bar laid out once and moved by clip-path, opacity and transform only;
  the owner, 2026-11-01), the keyboard pushing up only the glass typed in; the Library's types in three layers, records
  named by their type; the record Edit sheet and Classical's why; Health as Settings, Disc numbers, missing tracks many
  at once; one way to change files ([0092](decisions/0092-one-way-to-change-files.md)) with types into files its first
  writer, and the tag writer that keeps every field; STYLE read from the files again; Cratarr as an MCP server
  ([0090](decisions/0090-mcp-server.md)); test and development data ([0093](decisions/0093-test-and-development-data.md)).
  ([whats-new-0.13](whats-new-0.13.md).)
- **0.14: references and the intake** (the owner, 2026-11-01: "this comes next"): MusicBrainz, Discogs and Bandcamp as
  references, the checks without one, the hand check's intake card, one Health item for references (Soulseek in
  Cratarr, below). With it, the other writers onto 0092's path (Health's fixes, Renumber, the tempo, the covers, the
  length fixes, single-track intake), each losing its own Undo ([the map](writers-onto-0092.md)).
  And all of what the owner decided on 2026-11-02 ("All in 0.14"): taste in sequences and lists
  ([0094](decisions/0094-taste-in-sequences-and-lists.md): one taste per account, Best match on every list, Your taste
  as a radio mode, skips keeping tracks away gently, one radio sheet wherever radio starts, own orders named); one way to
  say nothing ([0095](decisions/0095-one-way-to-say-nothing.md)); a library's quality limit
  ([0096](decisions/0096-libraries-equal.md)'s amendment); energy and danceability on the library's 0–100% in the
  profile, the filter's window lit on the panel's bars (the sheet unchanged); a record's own Health problems on its page;
  and, suggested, the tagged total ("05/06") for missing tracks without a reference. Added the same day: faster
  previews (the image built on the tests' compiled parts) and a dependency check (duplicate crate versions, ts-rs out of
  the server); the settings' section Resets from the panel's one declaration, opening its sheet; the choice controls
  streamlined (the owner, 2026-11-02, as the preview.4 review decided: one row everywhere, one of many, several, a
  switch or a value, rows never carrying a pill); the global search
  button always an extension on the back layer; the false "no sound" on some MP3s (the dev page's player reconnecting every 3 s left out: no one
  could see it again, the owner included). Moved to 0.16 with 0096 (the owner, 2026-11-02): a playlist's Sync into "Shared with other users" and
  Repair into Health (both rework the .m3u files 0096 replaces), and Health's check for records above their library's
  quality limit (the limit itself for moves and Keep stays in 0.14). Previews on staging along the way. ([whats-new-0.14](whats-new-0.14.md).)
- **0.15: modes per server** (the External library below, taken up) **and scrobbling from the device**
  ([0091](decisions/0091-scrobbling-from-the-device.md)). With them, **Soulseek in Cratarr, Lidarr staying** (the
  owner, 2026-11-02, below): soularr goes; Cratarr fetches whole albums from Soulseek its own way (Lidarr's wanted
  albums through its API and Cratarr's own Requests, each searched by its release recording by recording, a folder
  chosen by its tracks, every file gated), into Incoming for the intake card; Lidarr keeps watching artists and
  grabbing torrents. Cratarr's mode is Cratarr's own library, its libraries still
  switched on and off within it; each other server (a Jellyfin, a Subsonic server) is a mode of its own, with its
  library and playlists, search and Cratarr's players, but no crates, Discover, Insights or Requests. The owner's
  design choices: [0098](decisions/0098-modes-per-server.md) (2026-11-03: in the app, as Local; no second Cratarr
  server in 0.15).
  With it, rhythm (the owner, 2026-11-02): four on the floor as a score (the share of beats a kick lands on, in the sub
  and bass bands' percussive hits, against the strikes between them) and the time signature from Beat this!'s
  downbeats (4/4, 3/4; odd meters less sure); measured in the analysis from then on, the library measured again at
  night (AudioMuse paused); conditions in the filter and the crate, words in the crate sentence ("four on the floor",
  "4/4"). The device index (0.16) carries them. And the playlist map's fourth closeness signal, Sound (the owner, 2026-11-02): playlists compared
  by their profiles (energy, danceability, tempo, the moods), so two that share no track or artist sit close when they
  sound alike; a switch beside shared tracks, genre and artists.
- **0.16: each user's own music** ([0084](decisions/0084-each-users-own-music.md)), **the whole library's index on the
  device, and crates and the queue on the device** (offline first's fourth and fifth steps, one version: the owner,
  2026-11-02, "17 sounds like 16 work"); settled in [0101](decisions/0101-own-music-one-track-id-library-on-the-device.md). Then all servers in one library, on the device (below), as a view over the
  modes' indexes.
- **Beside these, where there is room:** other versions not in the library (with Discover); matching versions by title
  and composer where MusicBrainz has no work for them.
- **Offline first** ([0085](decisions/0085-offline-first.md)), five steps each useful on its own: the apps carry their
  own pages (0.11, done); Downloads an offline shelf, and nothing done offline lost, playlist edits included (0.12,
  done); the whole library's index on the device, and crates and the queue on the device (0.16).
- **Then:** one tape engine ([0075](decisions/0075-one-tape-engine.md)), as the spike above turns out.

## Soulseek in Cratarr, not through Lidarr (explored 2026-10-31)

The owner: "I want to explore whether it's time we left SoulSeek in the hands of Cratarr altogether instead of using
Lidarr. But I'm not sure about this." Decided 2026-11-02: the first way below, *Cratarr does Soulseek, Lidarr stays*,
in 0.15. Only the bulk intake of missing
tracks below is being built (the owner, 2026-10-31: "Bulk search, you approve").

**What happens now.** Lidarr keeps the wanted list (its monitored artists and albums, Cratarr's Requests among them)
and grabs through qBittorrent and, via soularr, through slskd. soularr picks a whole folder from one Soulseek user by
its name and hands it to Lidarr, which imports what lands, unchecked. Its 72 hours to 2026-10-31: about 40 albums
imported; 34 albums failed outright after retries (half of them Brahms: "Ein deutsches Requiem op. 45" and "A German
Requiem" are one work under many names), 6,869 enqueues and 945 folder looks refused by users gone or unwilling, 109
downloads broken off. Cratarr's own Requests did fine: all 82 approved arrived, within six days.

**What Cratarr would do instead.** Search by MusicBrainz release, recording by recording, as single-track intake
(0080) does now: a folder chosen because its files are the release's tracks (title and length), not because its name
looks right; every file gated (decoded whole, the recording's length, AcoustID at 0.9); into Incoming (0034), where
the owner keeps it, never straight into Music. The administrator approves what is fetched, many at once (the bulk
intake below), so March 2026's unattended batches cannot happen again.

**Two ways.**

- *Cratarr does Soulseek, Lidarr stays.* soularr goes. Cratarr takes Lidarr's wanted albums (Lidarr's API) and its own
  Requests, and fetches them from Soulseek its way. Lidarr keeps watching artists for new releases and grabbing
  torrents. The smaller step, and the same engine as the bulk intake of missing tracks.
- *Cratarr replaces Lidarr.* Cratarr also watches artists (MusicBrainz's new releases) and grabs torrents. Lidarr and
  soularr go. Several versions of work; Lidarr does those two things well.

**Catches.**

- Soulseek manners: one search at a time, paced; never a chat room (the owner's rule); slskd's own limits (its 409s
  when searches pile up).
- AcoustID's rate (three lookups a second) paces the gate on a big batch.
- Lidarr's own folder (`lidarr-music`, 813 albums) is not in Health's checks or the missing-tracks pass, which look
  at Music only; if Cratarr fetches, where its albums land decides what Health sees.
- Found while looking: Cratarr's analysis says "no sound" for Roll the Dice (2010), whose MP3s play fine (−15 dB);
  the decoder gets no frames from them (a large cover in the tag, perhaps). To be fixed on its own.

**Next after 0.13 (the owner, 2026-11-01: "Yes this comes next, first we move to release").** Searching Soulseek
needs no MusicBrainz; judging what was found does: a reference says which tracks and lengths the release has.

- *Three references, in turn:* MusicBrainz, then Discogs (its track lists, not only its genres), then Bandcamp (an
  album page's own track list and lengths, as Cratarr already reads it for playing, 0071; strong for independent,
  electronic and digital-only releases). One that agrees on the track count and each length is the target. Bandcamp
  is found through its search box's answers only, so a match counts only where the tracks agree; Cratarr keeps its
  own link to the page.
- *Checked without a reference:* each file decoded whole, lossless where it says so, no clicks or clipping; track
  numbers without gaps or doubles; one album, album artist and year in every file; the folder's name as the tags
  say; every track from one Soulseek user's folder (a mix was March 2026's mark); AcoustID per track (the right
  recording under its title); already in the library or not.
- *The hand check, where no reference agrees:* one intake card per arrival on the review sheet's pattern (0092):
  cover, where it came from, format, the tracks with their lengths, each check green, amber or red with its one
  sentence, and per track a small waveform and play buttons for its start, middle and end (seconds per album, not
  a whole listen). Keep, Replace or Delete; nothing moves or is written before that yes. All green with AcoustID
  agreeing: offered with Keep ticked; amber or red: left open with its reason. A record checked by hand is its own
  reference for its type and year after.
- *One Health item for the references of every indexer* (the owner, 2026-11-01): "References", across MusicBrainz,
  Discogs, Bandcamp and AcoustID, never one item a source. Its entries: a record no reference agrees with (its
  type, year and completeness rest on its files alone; paste a link, or "checked by hand", the record then its own
  reference); references that disagree (track count, lengths, year side by side; the owner picks the one that
  counts); a reference found by guessing (Bandcamp's search box, MusicBrainz without an id in the files), shown
  once to confirm. Open, a record shows a row a source, agreeing, missing or differing, and what each says. What
  would change the files goes through the review sheet (0092).
- *CLAP, maybe, never with certainty* (the owner, 2026-11-01: "possibly but not with any certainty"): not a reference
  (it knows a sound, not a track list), at most an amber hint where AcoustID knows nothing: a track that sounds
  unlike its album, a "Classical" that sounds like house. Never red, never blocking; only if AcoustID turns out to
  miss too much. AudioMuse runs after import, so in Health, not on the intake card.
- Mockups of the intake card first (632), then the server's checks and references (a5).
- *All of it in 0.14* (the owner, 2026-11-02): Discogs' and Bandcamp's track lists and lengths as references, not only
  MusicBrainz's; a record checked by hand its own reference for its type and year; references that disagree side by
  side, and one found by guessing confirmed once; where a record came from for Lidarr and Bandcamp too.

**From SoulSync and DroppedNeedle (reviewed 2026-11-02, the HomeAssistant session, placed by 632).** The owner's rule:
0.14 stays light, only truly helpful bug fixes; the rest into 0.15 or 0.16, 0.17 only for a very good reason. Never
anything in without the owner's tick (0080); never a chat room.

- *0.14: nothing.* gapfill's two bugs are not Cratarr's: a refused enqueue fails the item at once (slskd's status, its
  failures, no batch: `slskd::fetch`), and a transfer slskd dropped fails it at the next read, every 2 s (the batch
  gone or empty is an error in `track_fetch::follow`), not after the 45 minutes. What gapfill guards, Cratarr's add
  already does: never overwriting (a file there already refuses), decoding whole, the length measured on the file
  against the recording's, AcoustID, copied in under a hidden name then renamed (no move across filesystems half
  done), a replaced track to the trash first and an Undo.
- *0.15, with the bulk intake (0096: a library's fetcher is a setting):* on to the next candidate when a peer refuses,
  errs or keeps it queued too long (its own queued limit, now one 30 minutes); a read of slskd that fails once is not
  the end of a fetch (today it refuses it and leaves slskd's download running, its file in slskd's folder); the
  intake's state on disk (in memory today: a restart forgets it); the peers that failed remembered per record; a
  ladder of searches (record and year, the title stripped, the main artist only, wildcards for artists Soulseek's
  server filters, none twice); a ladder of retries for missing tracks (none after 0 or 1 failures, 4 hours after 2, a
  day after 3, a week after 4 and more; never given up; Search now overrides; a time that cannot be read counts as
  due); each file set aside with why.
- *0.16, with each user's own music (0084):* the intake per library, each user's own and their room. Maybe a whole
  track's spectrum beside the 16 kHz gate (SoulSync's own detector is too crude to copy).
- *Not planned:* other sources (Usenet, streaming).
- To read again when building: DroppedNeedle's thin slskd client (429: one search or enqueue at a time; a circuit
  breaker; a test that keeps Soulseek's protocol out of the app) and SoulSync's retry backoff and quarantine (a JSON
  note beside each set-aside file).

## External library

Taken up for 0.12 as modes per server (the runway above). What follows is the plan as it stood.

Another music server's library played in Cratarr's app directly: an OpenSubsonic server, a Jellyfin, through the same
kind of adapters Cratarr's own sources use (0005).

- Waits for: the thin-client model. Today another server's music reaches the app only through Cratarr's server, as one
  of its sources.
- Catches:
  - The app would need its own adapters, its own sign-in for each server, and a library view that isn't a server page.
  - Queues, Play On and the history are held by Cratarr's server (0033). A track that only the phone can reach can't be
    handed over to another player, and its plays are not counted where the rest are.
  - Crates, playlists and analysis (0024, 0002, 0035) work on what the server catalogues. An external track would have
    no tempo, loudness or waveform unless the phone analysed it.
- What is new about it (2026-10-28): Cratarr's server already reads Jellyfin and Subsonic sources (0005, 0032), but
  those name files Cratarr sees on its own disks. An external library is music Cratarr cannot see as files, such as a
  friend's Navidrome, or one's own Jellyfin in another house.
- Two shapes (the owner, 2026-10-28: "just exploring for now", nothing chosen):
  - **A. Through Cratarr's server.** The other server is a remote source. The server reads its catalogue through its
    API and relays its streams (Subsonic `stream`, Jellyfin's universal audio). Everything that needs the server keeps
    working: mixed queues, Play On, Home Assistant, the history and scrobbles, crates over its tags, downloads, Radio
    and Autoplay. Waveforms, tempo and loudness could come from streaming each track once (opt-in, it costs bandwidth).
    The cost is two trips for the music (the other server to the NAS, then to the device), which matters on mobile data
    away from home. It keeps the thin-client model ("server first").
  - **B. In the app directly** (the plan as first written). The app has its own adapters and a sign-in per server,
    in a mode of its own, as local mode (0052) is. It works where only the phone reaches that server. It loses what
    local mode loses: Play On handover, Cratarr's history, crates, analysis, Home Assistant. It also needs library
    views that are not the server's pages.
- Settled (the owner, 2026-10-28): "because of our heavy feature set, we will offer extra servers only as modes". No
  combined library across servers, ever: each extra server is a mode of its own.
- The owner's lean before that (the same day, after "what fundamentally blocks one combined library?"): **different modes**, each
  server its own mode beside Cratarr's and Local, never mixed. That avoids matching one album across servers, mixing
  each server's user data, and queues holding tracks the next player cannot reach. Still open: whether a mode goes
  through Cratarr's server (shape A, with its queues, Play On, history and Home Assistant) or only through the app
  (shape B).
- Many servers, each its own mode (the owner, 2026-10-28): "possibly 2 Cratarr servers, 1 Jellyfin and also 1 local
  (never more than 1 local)", each separate.
  - The app holds the list, with a sign-in per server, and the mode switcher shows it (as Cratarr / Local now):
    in that example four entries, switched between as the two are now.
  - A Cratarr server's mode is that server's own pages, with its queues, Play On, history and Home Assistant. Each is
    its own origin, so their kept copies and storage never mix.
  - One queue per mode, and one mode sounds at a time: switching pauses the other, as Local does. Downloads are kept
    per server.
  - Catches:
    - A Jellyfin has no Cratarr pages. Its mode needs the app's own copy of the page with an adapter (shape B), which
      also lets the app work with no Cratarr server at all.
    - Two Cratarr servers may run different versions: the app's bridge must hold with either.
    - Settings split: the device's own (output, Start and stop, the tape, levelling) shared; Home, appearance per
      server.
    - The app is a Play On player on each Cratarr server: a play started from one switches to its mode.
    - Android Auto browses the current mode, or offers the modes as top folders.
    - The installed PWA stays one server's: it cannot switch origins or read the device's files.
- Open either way:
  - Whose favourites and plays they are: Cratarr's user data, the other server's stars mirrored (as Jellyfin's), or
    both.
  - Playlists are `.m3u` files of paths: an external track needs an entry form of its own.
  - Tags: only what the other server's API gives (genres, year, sometimes BPM).

### All servers in one library, on the device (explored 2026-10-31)

The owner, 2026-10-31, on the settled "modes only": the point of a merged library is one person with several unrelated
servers (a friend's Navidrome, one's own Jellyfin elsewhere, a Cratarr), merged on their own device, where no one
server can reach the others. Not decided; written down as the step after modes.

- How: the app signs in to each server and keeps each library's index on the device (a Cratarr server's by its change
  log, 0085's third step; a Jellyfin's or a Subsonic server's by their APIs). "All servers" is one more mode, the union
  of the modes' indexes, each record marked with its server's dot. Search, the filter and crates run on the device
  over it (the shared Rust core, the matcher); the queue is the device's (0085's fourth step); each track streams from
  its own server, and what is done to it (a favourite, a rating, a play, a playlist edit) goes back to its server.
- Graceful degradation (the owner's rule: nothing shown that cannot be consumed, nothing offered that cannot be
  upheld): each track carries what its server can do; its menu offers that, and a selection across servers what all
  of them can.
- What a track from another server goes without:
  - handing over: Play On, groups, Home Assistant, a server's queue for Android Auto hold only a Cratarr server's own
    tracks; a queue that mixes servers plays on this device, and offers no Play On;
  - the sound: Sounds like, Radio, Autoplay, the similar lists, AudioMuse's facts and the tempo; crates on those match
    only analysed tracks. Within its own server, that server's mix can stand in (Subsonic's similar songs, Jellyfin's
    Instant Mix), never across servers;
  - waveforms, snippets and the tape's own view until it has played here (the device can draw them as it plays);
  - one history: each play goes to its own server, so Cratarr's Insights, its week and its graves see Cratarr's tracks;
  - playlists across servers: no server holds one, so those are the device's alone;
  - one copy: an album on two servers shows twice, folded into one where both give its MusicBrainz release;
  - the library's care: Health, Discover, Requests, release types, Classical and its works, the Edit sheet.
- Only in the apps (Android, desktop): a browser page holds one origin's sign-in.
- It ends the thin client in that mode: the app answers the page's library itself, from its indexes.
- Order: modes per server first (each needs its adapter, its sign-in and its index on the device); then All servers
  as a view over them, after the device index and beside the queue moving to the device.

**Scrobbling from the device** (the owner, 2026-10-31: "scrobbling should indeed move to the device! This also makes
more sense privacy wise"): the device that plays a track tells ListenBrainz and Last.fm itself, its own queue of them
kept while offline (Last.fm takes them up to 14 days late, ListenBrainz any time). So it works the same for every
server's tracks, and the server never sends anyone's listening out nor holds the services' keys. Today the server
does it (0068's outbox, Connections). Settled the same day (the owner, question set), for 0.13 with modes:
- Each user's choice: **from each device** (the start) or **through the server**, to sign in to a service once and
  have it everywhere. Through the server, the devices hand it their plays of every server's tracks (the track's
  artist, title, album and when), which it scrobbles as now.
- Subsonic apps and Kodi, which play from Cratarr without Cratarr's app, follow the user's choice: through the server,
  it scrobbles them as now; from each device, the server holds no keys, and those apps scrobble themselves.
- Catches: from each device, a sign-in to each service on each one; a play told once, by the player that sounded it (a
  group's leader, not its followers); the switch between the two neither losing a play nor telling one twice.

## Local library

The phone's own music files, played in Cratarr's app beside the server's.

- Taken up (2026-10-19) as [0052](decisions/0052-local-library.md): a local mode of the app, never mixed with Cratarr's
  music. What follows is the plan as it stood.

- Waits for: the thin-client model, as above.
- Catches:
  - Reading the phone's files needs Android's media permission and an index of its own on the phone.
  - The same split as the external library: queues, Play On, history and crates live on the server, the files on the
    phone. Mixing both in one queue means a queue the server can't fully play elsewhere.
  - Not the same as kept tracks (0038): those are the server's tracks copied to the phone, and stay the server's.

## CarPlay

Cratarr's player in the car, on an iPhone.

- Waits for: an app on iOS. Cratarr has an Android app only (0031).
- Catches:
  - CarPlay shows an audio app through Apple's own list and now-playing templates, not a web page. The thin client's
    pages can't be its face in the car: the browsing would be native.
  - An audio app on CarPlay needs Apple's CarPlay entitlement for it, asked for and granted per app.
  - Android Auto, the counterpart on Android, was taken out of the app with the rest of Jellyfin's extras (0031).
    It is designed again and waits for a car or head unit that runs it (0055: the owner's tabs, voice and start; the
    car's JVC unit does not), and would come before iOS: Android Auto browses through a media library service
    (Media3), the kind of service the app's player already runs in.

## A chosen output while following a group

This device's chosen output (0033, amended 2026-10-19) applies when it plays alone or leads a group, not when it follows
one: then Snapcast's own player makes the sound, and the Snapdroid build Cratarr ships (v0.29.0.2) opens its Oboe stream
without a device. The owner (2026-10-19): "for later, if ever".

- What it would take, as looked at then:
  - The change is small: snapcast's `client/player/oboe_player.cpp` (v0.29.0) builds its stream with no `setDeviceId`;
    passing the `--soundcard` option's index there (and the app giving it the chosen output's id, restarting the player
    when the choice changes, as it does for a group change) is about ten lines.
  - The work is building snapclient ourselves instead of taking Snapdroid's: its build is Gradle with CMake 3.22.1 and
    NDK 27.2, snapcast as a submodule, and eight prebuilt dependency AARs from badaix/snapcast-deps v0.29.0 (boost,
    flac, oboe, ogg, opus, soxr, tremor, vorbis). Building arm64-v8a only would do for the tablet and the phone.
- Catches: GPL-3.0, so the patch stays in the repo with the source offer; our own build to keep up with Snapcast (or the
  patch offered upstream); a test by ear on a device following a group.

## One similarity for playlists and crates

Radio, Autoplay and "more like this" each found similar music their own way from a playlist, a crate or a track
(0036's profile: shared playlists, artists, genres, tags, era and AudioMuse's sound, 0054). The owner (2026-10-23): "we
should consolidate playlist and crate similarity into a unified module or path later on. We'll handle this after all
AudioMuse analysis is done."

- Done in 74.x (2026-10-23): one path, `similarity` (signals, rarity, middle and spread, the listener's Similar by),
  for the radio, Sounds like (a track's, an album's, your week's), and the similar albums, playlists and crates at a
  list's foot; the radio and the crates read a track from the same facts (`crates::Facts::like`).
- Left, and no longer waiting: AudioMuse's analysis of the whole library (with CLAP) finished on 2026-10-29 and is in
  Cratarr (31,687 tracks): weighing the signals (all equal now, or the listener's order), CLAP text search (the owner:
  "Yes, later", a design talk first), steering the radio by mood and energy, the closeness graph last.
- Seen so far: a crate's Start radio seeds from 300 of its tracks (drawn by the server since 73.2) and leaves the seed
  out; a broad crate (Unlisted, 16,000 tracks) makes a blurry profile. Play as radio (only the crate's tracks) is a
  different path again.

## AudioMuse-AI inside Cratarr, for bigger servers

AudioMuse-AI's analysis (0054) comes from separate software: its own containers and Postgres on a stronger computer,
its results handed over as a file. The owner (2026-10-24): having Beat This! and AudioMuse "opt in integrated would be
nice though for people running bigger servers". Beat This! has that now (`CRATARR_BEAT_THIS=on`, 0049's amendment of
2026-10-24). AudioMuse-AI is a larger step.

- What it would take: its models (MusiCNN for style and the sounds-like vector, CLAP for moods) run by the server
  itself, through tract or an ONNX runtime as Beat This! is, a file at a time behind the analysis' single turn, and
  only when asked for (an environment setting, off by default).
- Why it waits: CLAP alone needs far more memory and time per track than the NAS can give (a 768 MB server already
  could not hold Beat This!'s 650 MB); the models' licences and sizes have to be checked; the separate route works.
- The catch: the results must match the separate route's, so a library analysed one way and new music the other way
  stay comparable.

## Home offline, and Home in local mode

Home's sections are the server's (the owner, 2026-10-24: chosen there under one set of rules, no album without a cover,
no artist without a biography). Two cases are left without one, and the owner accepts the experience differing where
the limits are real, as long as it degrades gracefully:

- **Offline, with a server:** the app keeps the last Home it was given and shows it without a connection, what is not
  on the device marked and not playable. One implementation, the server's; the device only remembers.
- **Local mode (0052):** a different library, the phone's own, which the server never sees, so nothing it said
  applies. A smaller Home of its own, worked out on the device from what the device knows: the newest by date added,
  what was played here, favourites. The moment, Grave digging, Sounds like your week and Artists to explore need the
  analysis, the listening history or biographies, which only the server has: left out. In Settings › Home they show
  unavailable there, not switched off (the owner, 2026-10-24): greyed, "Needs your server", not to be switched on in
  local mode, and the account's own choice for them kept, so with the server again Home is as it was chosen.

## Requests: Want for everyone, in a queue (0.9)

Discover's Want sends an album to Lidarr (0069). The owner (2026-10-24): for someone not an administrator it should not
reach Lidarr directly, but go into a queue "the way Seer works"; to come in 0.9. Until then Want is an administrator's
(the server refuses anyone else's), shown unavailable to others.

Overseerr's and Jellyseerr's model (MIT; their code is TypeScript for films and series through Radarr and Sonarr, so
the design is taken, not the code, and they are credited): permissions per account (Request, Auto-approve, Manage
requests); a request's own status (pending, approved, declined with a reason) beside the album's (requested, being
fetched, partly there, in the library), which the requester sees on the album; the administrators' queue (approve,
decline, many at once), "3 requests waiting" in Health; limits per account if wanted (so many a week).

The owner, the same day, for 0.9 too: "surface the monitor of Lidarr in Cratarr to see what is currently being
monitored". Lidarr keeps the monitored albums it has not found yet (its Wanted, `/api/v1/wanted/missing`) and what it
is fetching now (`/api/v1/queue`): shown in Cratarr beside the requests ("Lidarr is looking for these"), each with how
far it is, so the queue and Lidarr's own watch read as one.

What Lidarr is for Cratarr (the owner, 2026-10-24): an acquisition engine, a want list on MusicBrainz plus a
search-and-import pipeline that drops albums into Incoming; Cratarr is already the curator around it (the library,
Incoming's Keep, Delete and Replace, upgrades, covers, Discover deciding what is wanted). Music also comes from other
sources than Soulseek, so Lidarr stays. The owner liked the intermediate step: in 0.9 **Cratarr keeps the want list and
each request's status itself, and Lidarr is one way of carrying a request out**, behind an interface another could take
(slskd directly, say) without the requests, their queue or their statuses changing.

## Listening before Want: Bandcamp, and YouTube

The owner, 2026-10-24, after first deciding against it and then reading GitHub's rules (no rule against software
using another service's public pages; Bandcamp's streams carry no copy protection; NewPipe, Bandcamp support
included, has long been on GitHub): "I do want to include this. Most definitely. Even possibly the entire NewPipe
extractor for YouTube too, unless YouTube fights too hard." A Discover suggestion's tracks play as a preview in
Cratarr's own player, never counted as plays nor scrobbled. Bandcamp: each album page lists its tracks' streams
(NewPipeExtractor's Bandcamp part, GPL-3.0-or-later, ported to the server with its notice; a browser may not read
the pages, the server can). YouTube changes its scrambling and checks often (NewPipe and yt-dlp release fixes every
few weeks) and NewPipeExtractor is Java (too heavy for the NAS's 768 MB): proposed through yt-dlp as an optional
service of its own, as AudioMuse is, updated apart from Cratarr. A decision (0071) to be written first.

## Genre territories on Insights' Map

The owner, 2026-10-24, from a ChatGPT conversation they shared: the playlists' map (Insights › Map, decision 0065)
gets genre territories, like a political map. Each genre is an area drawn around the playlists where that genre makes
up much of the music, so islands, holes and enclaves (a pocket of electronic inside rock) come out by themselves. A
mock-up was made (artifact UJ4zeUZEZPw2pcUie2D25B; geometry from scratchpad `territories/geo.py`: each genre's outline
is the 0.5 contour, by marching squares, of a Gaussian field weighted by the genre's share in each playlist, only
playlists with a share of 0.25 or more counting). The owner: "Yes, it looks fine." Built in 0.9.8 (2026-10-26,
lib/territories; the per-playlist `shares`): the Map's points no longer move (the owner: "no node movement, a real
map"; pan and zoom stay), the six genres follow the Map's filters, a legend tap shows one, a tap inside one highlights
its playlists, and the Map fills its page with nothing to scroll.

Decided (the owner, 2026-10-24): a playlist's genres come from its tracks' genre tags (counted per playlist; not
MusicBrainz's genres, not AudioMuse's styles); the territories are a switch on the Map, off until turned on; the top
six genres are drawn, each its own colour (the palette's categorical order), the rest no territory.

## Parked

Liked, not taken up:

- The visuals' VJ ideas: fused passes, a blur pyramid, the loop at a lower resolution, layers at half rate, an A/B
  crossfade between racks, ISF shaders, a meter of each module's cost.
- Discover's Weekly (a handful of records each week) and a pick of the day.

Shelved by the owner: a live DJ mix between tracks (Auto DJ), and an MCP server in the desktop app.
