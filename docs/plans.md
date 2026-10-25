# Plans

Big ideas that are not decided and not being built. Each entry gives what it is, why it waits, and the catches known so
far. A plan that is taken up becomes a decision in [decisions](decisions) first, and its entry here points to it.

Most of these wait for the same reason. Cratarr is a **thin client**: the server holds the library, the queues (0033)
and the pages, and the app shows the server's pages, with a native player under them (0031). The app keeps tracks for
offline play (0038), but it only knows what the server tells it. Each plan below asks the phone, or a car, to know
something the server doesn't.

## External library

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

What Lidarr is for Cratarr (the owner and a5, 2026-10-24): an acquisition engine, a want list on MusicBrainz plus a
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
playlists with a share of 0.25 or more counting). The owner: "Yes, it looks fine." Not built, and no decision written
yet. Recorded late: the mock-up was accepted but never written down here, so no session had a trace of it.

Decided (the owner, 2026-10-24): a playlist's genres come from its tracks' genre tags (counted per playlist; not
MusicBrainz's genres, not AudioMuse's styles); the territories are a switch on the Map, off until turned on; the top
six genres are drawn, each its own colour (the palette's categorical order), the rest no territory.

## 0.9.3: deep search everywhere, and global search

Moved from 0.9.2 by the owner, 2026-10-25: 0.9.2 ships without it.

The owner, 2026-10-25: "Deep Search Everywhere 0.9.2, together with Global Search. Same thing, really." Every page's
search finds deep (a playlist by its tracks, a crate by what it holds, an artist by its albums), and one global search
finds across them all; both by the one pattern (ui-conventions, "Search: the page finds, the panel narrows"): what
matches shows, and the item opened shows what of it matches the same words. Needs a server search over the library and
the playlists' and crates' contents (one call, its results by kind).

## 0.9.2: one copy of each album, related artists to explore

The owner, 2026-10-25, looking at Dominik Eulberg's artist page on the phone: "What's up with these duplicate albums? And
why do I not see anything in the health about this?" Bionik and Heimische Gefilde showed twice: Lidarr's copies in
Incoming, left there when the FLAC upgrade of 2026-10-12 took them into Music. 24 such albums; 17 the same release with
the same tracks, Air's Moon Safari with one track more in Incoming, 6 another edition.

- **Health: albums in Music and Incoming at once** (the owner chose it): by MusicBrainz release, else by name and artist
  as the import's matcher reads them; each says which (the same release, or another edition, with both track counts)
  and opens its Incoming page, where Delete or Replace settles it.
- **Similar albums leave out the album's own artist** (the owner: the albums in More from the artist should not be
  similar albums too): every album by the artist, not only the ones More from shows; More from leads to all of them.
- **Related artists to explore on an artist page** (the owner: "recommended artists, for example. related artists to
  explore. That's the best."): one row of artists, as Home's Artists to explore, ranked from several sources: who
  ListenBrainz finds listened to alongside (similar artists, by MusicBrainz id), MusicBrainz's members, projects and
  collaborators, label mates (the labels MusicBrainz gives the artist's releases, and a label's own Bandcamp roster),
  and Bandcamp's "you may also like" on the artist's album pages. Artists that play on Bandcamp first (the owner:
  "prioritize Bandcamp playable artists"), so a new one can be heard at once. One in the library opens its artist page;
  a new one its Bandcamp album in the album panel, as Discover's do. Looked up with the artist's other facts and kept;
  MusicBrainz and Bandcamp at their one request a second.
- **Discover's masonry** (the owner with claude-632, 2026-10-25; the page claude-632's): tiles keep their size (1×1 = 136
  px, a little under a shelf's 150 px cover), the width decides the columns, two rows at least. Each artist gets the
  smallest grid its suggestions fill, its seed tile in the middle with the artist's picture (tall over two rows); the
  suggestions packed outward from it, the most alike nearest, those that play on Bandcamp 2×1 above and below it and
  1×2 beside it. A first artist's grid grows only while its next suggestions stay close (about half the strongest
  match or more); width left over goes to a second artist by the same rule. The server gives each suggestion how alike
  it is to each artist it was found from (`alike`: Last.fm's match per seed).

## 0.9.3: built-in crates, and your crates on Home

The owner, 2026-10-25: Home's crates belong on the Crates page too, and one's own crates may come onto Home.

- **Built-in crates** (the owner chose the word): New for you, Grave digging and Sounds like your week (Home's shelves
  that are crates) are listed on the Crates page beside Unlisted, which is one already. Like Unlisted, none can be
  removed (no Delete, no Rename); each is marked as built-in. The crates' filter sheet gets a toggle, "Hide built-in
  crates".
- **Your crates on Home:** Settings › Home gets a switch at the top, under the section header, "Your crates on Home".
  On, each of one's own crates joins the sections list right below, to be ordered and switched on or off as every
  section is. On Home a crate shows as a shelf of its albums (as New for you does): its name as the header, a row of its
  albums' covers, › opening the crate, a cover its album, the shelf's play buttons as the others'.
- **The loved tracks not in the library, with their albums** (the owner, 2026-10-25: "please", after the list showed
  many without one): Last.fm keeps no album with a love, and a love is often kept under another of the track's artists
  (Alchemist's "Eggs on the Third Floor" is Action Bronson's on Rare Chandeliers). Each loved track's album is the one
  it was most scrobbled from under its own artist, else by its title alone where that title was scrobbled from one album
  only (not for a short, common title), else Last.fm's track info's (500 at most, four a second). The album found
  also matches the library's track by album and title, so the love becomes a favourite under whichever artist it was
  kept. A track loved under several of its artists is one row with all of them.
- **Pure breadcrumbs** (the owner, 2026-10-25: Requests, Downloads, Insights, Health, Settings; "2 at most. So the
  current and one level above."): every such panel's title is the level above › where one is (lib/Crumbs): Requests ›
  Waiting for an answer, Collection › Genres, Library › a Health problem (on an item's page: the problem › the item),
  Settings › Account (on a thing's page: Account › the thing): for Insights and Health the level above is the view's
  group (the owner chose it). The level above is a tap: a page's or a group's name closes the panel and shows that page
  wherever the panel was opened from (`upToPage`: global search's results will open anywhere); a part's name goes back
  to it. Health names its Library domain (it showed "library").
- **Discover in the bar where there is room** (the owner): the phone and tablet bar shows as many of the main pages as
  fit (a tab at least 72 px wide): four and More on a phone, all five with Discover on a tablet. The pages apart
  (Requests, Insights, Health, Settings) stay behind More: "requests is of the other category".
