# Decisions

Each record here is one decision about Cratarr: what was decided, why, and what follows from it (Status, Date, Context,
Decision, Consequences). A decision that changes later is amended in its own record, below the decision, or superseded
by a later record that says so in its header. A new decision takes the next number (`NNNN-slug.md`) and a line here.

- [0001: Scope and stack for version one](0001-stack-and-scope.md): accepted. Version one lists every playlist with the
  playing track's memberships marked and adds or removes that track; a Rust server with the Svelte page built into it,
  playlists kept in Jellyfin.
- [0002: Playlists are `.m3u` files; Jellyfin supplies what is playing](0002-playlists-are-m3u-files.md): accepted
  (supersedes the playlist handling in 0001). The `.m3u` files beside the music are the truth, read and edited in place
  with a backup; what plays comes from Jellyfin's activity log.
- [0003: Album covers are read from the album folder](0003-album-cover-endpoint.md): accepted. The playing track's cover
  is served from the image file in its album folder, cached by the browser, never from a path in the request.
- [0004: Connection settings can be changed from the page](0004-settings-page.md): accepted; the file format and the
  per-key overrides superseded by 0005. The connection settings are saved from the page into a file in the data folder,
  which wins over the environment, and only after the new connection is checked.
- [0005: Music sources are plugins; libraries are chosen, paths are found](0005-pluggable-sources.md): accepted; library
  roles revised by 0009; amended. Each music server is a source plugin compiled in that declares its own settings form;
  the core owns the playlist files and finds where each source's folders are.
- [0006: Players are their own concept, chosen per browser](0006-players.md): accepted. A player is a concept of its own
  across sources, and each browser chooses the player it follows (or Automatic), with no server-wide default.
- [0007: Playlists are exported on demand, as a download](0007-playlist-export.md): accepted; amended. Ticked playlists
  download as plain `.m3u`, optionally with their tracks, in one folder or combined into one playlist.
- [0008: A local catalogue of the music Cratarr can see](0008-catalogue.md): accepted; extra folders superseded by 0009.
  Cratarr keeps its own SQLite catalogue of every audio file and its tags, read from the disk, with each source's ids
  matched in the background.
- [0009: Uses are set once, on the server's libraries; the folders behind them are found](0009-library-uses.md):
  accepted. Each server library is Music, Incoming or Not used, set in one place; Cratarr finds the mounted folder
  behind it and says when it cannot see one (replacing a default of 0005 and 0008's extra folders).
- [0010: One writable mount; Cratarr tests where it can write](0010-writable-libraries.md): accepted; amended
  2026-10-13. One writable mount holds the music libraries; Cratarr writes only playlists and moved albums in Music and
  its trash and conversions in Incoming, making folders with their parent's permissions.
- [0011: Feature modules around a functional core; ports where things vary; events between
  features](0011-architecture.md): accepted. A modular monolith cut by feature: decisions as pure functions, ports where
  things vary, events and a journal between features.
- [0012: Playlists become the user's own Jellyfin playlists, kept in step with the .m3u files both
  ways](0012-own-jellyfin-playlists.md): accepted; its one switch and the move out of Music replaced by 0014's sync
  links. Each playlist is kept both as an `.m3u` file and as the chosen user's Jellyfin playlist, merged three-way in
  both directions.
- [0013: Keep and Replace: Incoming albums into Music](0013-keep-and-replace.md): accepted. Keep moves a new album from
  Incoming into Music, named by the library's template; Replace puts an upgrade in the place of the album it improves,
  track by track.
- [0014: Sync links: one playlist, the targets it is kept in](0014-sync-links.md): accepted; amended by 0025 and twice
  on 2026-10-15. Every playlist is one `.m3u` file, kept in step with the targets linked to it: a shared copy for
  everyone or an account's own Jellyfin playlist.
- [0015: Playlists by spoken name: Cratarr answers, the assistant plays](0015-playlists-by-spoken-name.md): accepted. A
  voice assistant asks Cratarr for a playlist by its spoken name, matched loosely but never guessed, and plays the
  tracks it gets back itself.
- [0016: Music's layout: one folder per album-artist credit, never a Lidarr root](0016-music-layout.md): accepted. Music
  is nested as album-artist credit, then album, compilations under `Various Artists/`, and is never one of Lidarr's root
  folders.
- [0017: Telling the media server about moved albums, and noticing when it stops
  listening](0017-telling-the-media-server.md): accepted. Moves are announced per album folder and the receiving library
  asked to look at once; a playing album is stopped and resumed around its move; a media server that stopped processing
  changes is noticed.
- [0018: The full-screen player fits itself to every size](0018-responsive-player.md): accepted; amended. The
  full-screen player measures its own controls and gives way in a fixed order, the cover negotiating its size, so it
  fits every screen.
- [0019: What the remote takes from Finamp's player](0019-remote-features-from-finamps-player.md): accepted; amended.
  From Finamp's player the remote takes info chips, a track menu, a queue view and the choice of output, local or
  remote.
- [0020: An album goes into the folder Music already has, whatever its capitals and accents](0020-artist-spelling.md):
  accepted. Keep and Replace follow Music's existing spelling of a folder whose name differs only in capitals and
  accents, so no case twins.
- [0021: Menus are bottom sheets, like Finamp's track menu](0021-menus-are-finamp-style-sheets.md): accepted. Every menu
  is one bottom-sheet component (`MenuSheet`) with the item at its top, modelled on Finamp's track menu.
- [0022: The local player](0022-the-local-player.md): accepted. The browser plays by itself, gapless through one
  continuous MSE stream of the original files, served by Cratarr without transcoding.
- [0023: One panel: the player and playlists](0023-one-panel-player-and-playlists.md): accepted; superseded in part by
  0025; amended. One panel shows the player or an opened playlist, stacked, with the mini-player standing in while the
  player is covered.
- [0024: Crates](0024-crates.md): accepted; amended. A crate is a dynamic playlist, a cross-section of the library
  worked out when looked at; Favourites and Unlisted are always there.
- [0025: The panel: a subject in tabs](0025-the-panel-a-subject-in-tabs.md): accepted; amended. The panel shows a
  subject: the playing track, or a selection of playlists in tabs (Tracks, Info, Export, Repair, Sync); an album or a
  crate too.
- [0026: One undo journal](0026-one-undo-journal.md): accepted. Every change to playlists, crates and Incoming goes
  through one journal and returns an undo token, replacing the older undos.
- [0027: One row in three sizes](0027-one-row-in-three-sizes.md): accepted. One row primitive in three sizes and one
  component per kind of thing, so a playlist, a file or a track looks the same in every list.
- [0028: Collection, and one album list](0028-collection-and-album-lists.md): accepted. A Collection page of the Music
  library's albums beside Incoming, both on one album list in rows or cover tiles.
- [0029: The AGPL licence](0029-agpl-licence.md): accepted; amended. Cratarr is AGPL-3.0-or-later, so GPL-3 and AGPL-3
  code may be used with its notices; the Android app stays GPL-2.0, as the Jellyfin app it came from.
- [0030: Cratarr serves the OpenSubsonic API](0030-cratarr-serves-opensubsonic.md): accepted; amended. Cratarr serves
  OpenSubsonic at `/rest` from a library database built from its catalogue, with code and patterns taken from Navidrome,
  gonic and others under their notices.
- [0031: The Android app and Play On](0031-android-app-and-play-on.md): accepted; amended. Cratarr's Android app, made
  from Jellyfin's, shows the pages in a WebView and plays natively; Play On lets Cratarr and Home Assistant command its
  players.
- [0032: Cratarr is its own source](0032-cratarr-is-its-own-source.md): accepted. A built-in `cratarr` source answers
  from Cratarr's own library, user data and players, so music needs no media server.
- [0033: Cratarr holds the queues](0033-cratarr-holds-the-queues.md): accepted; amended. Cratarr holds every player's
  queue as entries with ids and a version; edits name entries, not places; a player holds its own only while it cannot
  reach Cratarr.
- [0034: One Library page, and moving albums between libraries](0034-one-library-page-and-moves.md): accepted; amended.
  Incoming and Collection become one Library page showing each album's facts; Move to a library replaces Keep, and
  Delete uses each library's own trash.
- [0035: Audio analysis: waveform, tempo and loudness, worked out once on the server](0035-audio-analysis.md): accepted;
  amended. One low-priority background pass decodes each track once for its waveform, tempo and loudness, later its
  beats, bands and the visuals' signals.
- [0036: Radio: the music goes on, from the owner's own library](0036-radio.md): accepted (built 2026-10-16); amended.
  Radio is a setting of the queue that keeps the music going from the owner's library when the queue runs out; Radio and
  Autoplay were later split.
- [0037: Sync play: one queue on several of Cratarr's players at once](0037-sync-play.md): accepted (built over
  Snapcast); amended. A group of players plays one queue in time, its commands the group's, volume each player's own.
- [0038: Kept on this device: playlists, crates, albums and artists that stay in step
  offline](0038-kept-on-this-device.md): accepted; amended. Keep on this device is a switch on a playlist, crate, album
  or artist that the app then follows offline; Download stays in the menu as a snapshot.
- [0039: Cratarr's own sign-in page and sessions](0039-sign-in-page-and-sessions.md): accepted. Cratarr has its own
  sign-in page and per-device sessions, as OWASP advises, in place of HTTP Basic at the proxy; the app keeps no
  password.
- [0040: A directory (LDAP) asked first at sign-in](0040-directory-sign-in.md): accepted. Sign-in asks an LDAP directory
  first, when one is set, and Cratarr's own accounts after; the directory is the operator's to set, read-only in
  Settings.
- [0041: The tempo spread](0041-tempo-spread.md): accepted; amended. A tempo spread, tracks per 5-BPM band from 60 to
  180, shown in a playlist's Info and as the tempo filter's selector.
- [0042: Columns where there is room, and a sidebar that folds](0042-columns-and-a-folding-sidebar.md): accepted. Lists
  show columns in an order of priority as their own width allows, the order the user's; the navigation folds to a strip
  of icons.
- [0043: Sign-in everywhere](0043-sign-in-everywhere.md): accepted; amended. Cratarr requires a sign-in on every
  request, from anywhere: a session, an API key or a signed link, and nothing else.
- [0044: Folders, chosen and shown](0044-folders.md): accepted. Settings › Libraries is one list of chosen folders with
  their use and status; Add folder browses what Cratarr can see, and found folders are only suggested.
- [0045: Diagnostics: what happened, kept by Cratarr itself](0045-diagnostics.md): accepted; amended. Cratarr keeps a
  diagnostics log of its own (connections, refusals, sync play, imports, the app's log), to be read and downloaded.
- [0046: Visuals: each track's own fractal flame, full screen](0046-visuals.md): accepted; amended. A full-screen
  visuals mode in which each track has its own fractal flame driven by the music, grown by many amendments (bands,
  routes, sliders, snapshots, sync groups).
- [0047: Visuals: the flame's light in a fluid](0047-fluid.md): accepted; amended. The flame's light can be carried by a
  fluid the music stirs (stable fluids), from none, as before, to all, on one Fluid setting.
- [0048: Visuals: a complete randomisation, kept watchable](0048-governor.md): accepted. The dice roll a whole recipe
  from a seed, behaviour as well as looks, and measured flashes and motion keep the result watchable.
- [0049: The visuals' beat, downbeat and tempo from a local pulse](0049-pulse.md): accepted, amended 2026-10-19. The
  visuals' beats and downbeats come from Beat This!, run over the library, with the Tempo row faded by the pulse's
  strength meanwhile.
- [0050: Visuals 65: band envelopes, fewer sliders, an effects stage, Facets](0050-effects-stage.md): accepted; amended.
  Visuals 65: band envelopes and Presence, one Quality and one Haze in place of several sliders, a stage of picture
  effects, and Facets.
- [0051: Several users](0051-several-users.md): accepted; amended. Every member of the directory's music group gets an
  account with their own favourites, history, queues and settings; playlists are personal with a share option.
- [0052: Local mode, the device's own music](0052-local-library.md): accepted. The app plays either Cratarr or the
  device's own music from Android's media index, never mixed, each mode with its own queue.
- [0053: Covers inside the files, and next to them](0053-covers-inside-the-files.md): accepted. A cover lives inside
  every track and as an image file beside them; Keep and Replace bring new albums there; a stand-in shows where there is
  none.
- [0054: AudioMuse's analysis, taken in](0054-audiomuse-import.md): accepted. A task on the PC exports AudioMuse's
  energy, key, style, moods and similarity, which Cratarr takes in whole and shows, sorts and filters on.
- [0055: Android Auto, designed and waiting for a car that runs it](0055-android-auto.md): accepted as a design, not
  built. The player service becomes a media library service with four tabs (Crates, Playlists, Favourites, Queues),
  built when a car with Android Auto comes.
- [0056: The visuals as a patch: layers, modules, modulators and cables, morphing all the way](0056-patch.md): accepted;
  amended. The visuals become a patch: layers of modules, modulators cabled to any knob, every change morphed; it
  replaces how 0046's routes and settings are shown and kept.
- [0057: One engine: everything that changes the picture is a module or a cable you can see](0057-one-engine.md):
  accepted. One engine over typed ports: everything that changes the picture is a module, a cable or a macro shown in
  the graph and the rack, the Cratarr look a chain of them.
- [0058: One menu system: every object's menu opened the same way, its entries in the same
  groups](0058-one-menu-system.md): accepted. Every object's menu opens the same way and groups its entries the same
  way; the music app's menus are the reference for the visuals' too.
- [0059: One set of play rules, run where the library and the queue are](0059-one-set-of-play-rules.md): accepted. What
  a play queues, shuffle and the radio's lanes are decided by one pure Rust crate (`cratarr-rules`), run on the server
  and in the page alike.
- [0060: One set of sorts and filters per kind of item](0060-one-set-of-sorts-and-filters.md): accepted; amended. Order
  and direction in the list's bar, the look in View, every filter in the Filter sheet; one name per fact and one set of
  sorts and filters per kind of item.
- [0061: A filter is an unsaved crate on the list in view, any or all of its
  tracks](0061-filters-are-unsaved-crates.md): accepted. A filter is a crate's conditions, unsaved, on the list in view,
  with Any or All for items that hold tracks, remembered like sort.
- [0062: The app owns where its controls point; sounding here means controls
  here](0062-the-app-owns-where-its-controls-point.md): accepted. In the app, only the player service keeps where this
  device's controls point, and whatever starts sounding here brings the controls here.
- [0063: One health report, every domain's checks; the Health page shows it all, System status what matters
  most](0063-one-health-report.md): accepted. Every domain's checks give findings to one health report; the Health page
  shows it all, Home's System status the most important.
- [0064: One listening history: a log of every track heard](0064-one-listening-history.md): accepted; amended. A log of
  every track an account heard, with skips, kept forever and written through one function; the time before it filled
  from ListenBrainz.
- [0065: Insights: Collection · Listening · Map](0065-insights-collection-listening-map.md): accepted; amended
  2026-10-24. Insights in three views named for their question (Collection, Listening, Map); since 0.8 a page of views
  that open in the panel.
- [0066: One palette for both themes: a green, a red and an amber that hold on either
  page](0066-one-palette-for-both-themes.md): accepted. One green, one red (coral) and one amber (honey) for both
  themes, each holding at least 3 : 1 on both pages.
- [0067: One import: what you did elsewhere, from any source, matched once, previewed, undone](0067-one-import.md):
  accepted. One import for listening history, counts, favourites, ratings and playlists from any source, matched once,
  previewed and undoable.
- [0068: One way in for listening: written first, then told](0068-one-way-in-for-listening.md): accepted; amended. Every
  live play goes through `app::listening`: written first in one transaction with an outbox for the scrobbling services,
  then told on one channel.
- [0069: Discover: music outside the library, from what you listen to](0069-discover.md): accepted; amended. A Discover
  page of albums outside the library, from the account's ListenBrainz and Last.fm, each with why it is suggested.
- [0070: Requests: Want for everyone, in a queue Cratarr keeps](0070-requests.md): accepted. Want files a request in a
  queue Cratarr keeps, each with its status; an approved one is handed to a fetcher such as Lidarr.
- [0071: Listening before Want: Bandcamp previews in Cratarr's own player](0071-listening-before-want.md): accepted;
  amended. A Discover suggestion on Bandcamp plays as a preview in Cratarr's own player, found through MusicBrainz or
  Bandcamp's search, never downloaded.
- [0072: One search, structured: the pages', the panel's and the global one](0072-search.md): accepted. One search
  contract for the pages, the panel and the global search: the query read one way everywhere, and deep search within
  sets.
- [0073: Spotify as a Library source: what you keep there, what Cratarr has of it, and asking for the
  rest](0073-spotify-library.md): accepted. Each account's Spotify lists, from its own Spotify app or a data export, are
  kept by Cratarr and marked with what the library has; the rest can be requested, nothing plays on Spotify.
- [0074: Winding: the track heard backwards and forwards at speed, as a tape deck winds](0074-winding.md): accepted.
  Winding is the track's own stream backwards or forwards at speed, rendered by the server, each player adding its vinyl
  ramp; holding Previous or Next winds.
- [0075: One tape engine: a playhead with a signed speed, anything interrupting anything](0075-one-tape-engine.md):
  accepted as the North Star. Each player has one tape engine, a playhead with a signed speed; every control sets a
  target speed, so anything interrupts anything.
- [0076: Global search in three tiers: the library, what is known elsewhere, and
  beyond](0076-search-three-universes.md): accepted. Global search looks in the library, then in what Cratarr knows from
  elsewhere, then in MusicBrainz and Discogs live, each thing once.
- [0077: Macros are the rack's face; the dice play the face](0077-macros-the-racks-face.md): accepted. Every level of a
  rack has a few macros that reach one level down, and the dice roll macros only; supersedes 0057's Track Look.
- [0078: A desktop app: Cratarr's pages in a window of their own, Linux first](0078-a-desktop-app.md): accepted. A thin
  Electron app shows the server's pages in a window of its own, Linux first as a .deb from an APT repository.
- [0079: Listening along: following another listener, asking for a track, and a guest
  page](0079-follow-and-queue-requests.md): accepted. One may follow another user's player, opted in, from anywhere,
  kept in step by Cratarr; followers and guests at home ask for tracks the leader accepts or declines.
- [0080: Single-track intake: one track into an album, checked before it goes in](0080-single-track-intake.md):
  accepted; amended 2026-10-30. One track at a time from a Soulseek search or, since 0.9.19, a file in Cratarr's
  slskd folder, gated by a whole decode, the recording's length and AcoustID, then added to the album with Undo.
- [0081: Release types: MusicBrainz's release group, else the Discogs release that is the album](0081-release-types.md):
  accepted; amended 2026-10-30. An album's type is its MusicBrainz release group's, else its Discogs release's when
  that release fits the album's tracks or length; an EP's edition far longer than its group's first release is an
  Album.
- [0082: Classical as a kind of record, and a track's Other versions](0082-classical-and-other-versions.md): accepted.
  A record is classical by a genre gate and one confirmation (MusicBrainz works with performers, a composer confirmed
  elsewhere, or a mark); composers by MusicBrainz artist; each record in its primary kind by default; a track's other
  recordings of its work.
- [0083: Excluding an artist: not in this crate, or everywhere](0083-excluding-an-artist.md): accepted;
  amended 2026-10-30. Not in this crate adds "Artist is not" to the crate's own filter (a fixed crate's added
  conditions, per account); Exclude everywhere is one list per account that every pick reads, by any credit, as the
  Artist filter itself now matches; Discover's Not interested stays Discover's.
- [0084: Each user's own music](0084-each-users-own-music.md): accepted as a plan, for 0.13. Music, Lidarr's
  Incoming and each user's own music; wants and uploads go straight into one's own music within a GB allocation
  (shared-block copies free) and a disk floor; every library and Incoming switchable per user; export as a read-only
  network drive of one's own music and favourites (from outside too, per account).
- [0085: Offline first, in five steps](0085-offline-first.md): accepted as a plan, beside 0.11 to 0.14. The apps
  carry their own pages; Downloads an offline shelf; nothing done offline lost; the whole library's index on the
  device; crates and the queue on the device, through the shared Rust core. The merge rules are the owner's to set.
