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
- **0.12: modes per server** (the External library below, taken up). Cratarr's mode is Cratarr's own library, its
  libraries (Music, Incoming, each user's own) still switched on and off within it; each other server (a Jellyfin, a
  Subsonic server) is a mode of its own, with its library and playlists, search and Cratarr's players, but no crates,
  Discover, Insights or Requests. With it: Classical's works and movements (a composer, their works, each work's
  recordings, movements grouped under their work).
- **0.13: each user's own music** ([0084](decisions/0084-each-users-own-music.md)).
- **Beside these, where there is room:** other versions not in the library (with Discover); matching versions by title
  and composer where MusicBrainz has no work for them.
- **Offline first, beside these** ([0085](decisions/0085-offline-first.md)), five steps each useful on its own: the
  apps carry their own pages (beside 0.11); Downloads an offline shelf, and nothing done offline lost (beside 0.12);
  the whole library's index on the device (beside 0.13); crates and the queue on the device (0.14).
- **Then:** one tape engine ([0075](decisions/0075-one-tape-engine.md)), as the spike above turns out.

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
- Left, and it waits for AudioMuse's analysis of the whole library (with CLAP): weighing the signals (all equal now,
  or the listener's order), CLAP text search, steering the radio by mood and energy, the closeness graph last.
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
