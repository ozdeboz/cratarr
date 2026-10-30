> **Work in progress.** Cratarr changes every week. It is played every day on one home server, but pages, settings and
> data still move between versions: read a release's notes before updating, and keep a backup of its data folder.

# Cratarr

A self-hosted music server for the music you own. It plays your library in its web app, its Android and desktop
apps and any OpenSubsonic app, moves the music between them, and finds more to play in what you already have. Your
playlists stay plain `.m3u` files beside your music.

## Try it

There is no published release yet: the image, the apps and their downloads come with the first one. Until then,
build it from this repository (Docker is all it needs):

```sh
git clone https://github.com/ozdeboz/cratarr && cd cratarr
docker build -t cratarr .
mkdir -p ~/cratarr
docker run -d --name cratarr -p 8080:8080 --user "$(id -u):$(id -g)"   -v /path/to/your/music:/music -v ~/cratarr:/data   cratarr
```

Open `http://localhost:8080`, make the first account, and in Settings → Libraries choose your music folder. Cratarr
reads it once (covers, waveforms, tempo, loudness), and plays while it does.

Until that first account exists, anyone who reaches the server can make it, and act as its owner (as Navidrome's first
run). Make it before the server can be reached from outside your home.

The Android app and the desktop app are built from [android/](android) and [desktop/](desktop) (each folder says how);
give them your server's address. Once releases are published, each will carry a ready image (amd64 and arm64,
`ghcr.io/ozdeboz/cratarr`), the Android app, the desktop app for Linux (also from an APT repository) and
`SHA256SUMS` of them all.

## What it does

**Play it anywhere**
- In the browser (installable over HTTPS), the Android app (offline too, and with the screen locked) and the desktop
  app (a player that starts at login and keeps playing with its window closed); Subsonic and OpenSubsonic apps sign in
  with your Cratarr account ([0030](docs/decisions/0030-cratarr-serves-opensubsonic.md),
  [0078](docs/decisions/0078-a-desktop-app.md)).
- **Play On**: Cratarr holds every queue, so music moves from one player (a browser, a phone, a computer, a Kodi box)
  to another where it was, any player is controlled from any other, and several play one queue in step
  ([0033](docs/decisions/0033-cratarr-holds-the-queues.md), [0037](docs/decisions/0037-sync-play.md)).
- **Listen along**: follow another listener's player and ask for tracks, which they accept or decline in their queue;
  guests at home can ask too, from `/guest` ([0079](docs/decisions/0079-follow-and-queue-requests.md)).
- A tape deck's feel, if you want it: Start and stop that spin up and down, and winding through a track while you hold
  ([0074](docs/decisions/0074-winding.md)).
- Visuals: a full-screen picture that moves with the track, built from modules you can patch, and opening by itself
  when the screen is left alone if you like ([0046](docs/decisions/0046-visuals.md),
  [0057](docs/decisions/0057-one-engine.md)).

**Keep it in order**
- **Playlists are `.m3u` files** in your music folder, so any player reads them. Every change is backed up and can be
  undone ([0002](docs/decisions/0002-playlists-are-m3u-files.md), [0026](docs/decisions/0026-one-undo-journal.md)).
- **The Library** lists your records by kind (albums, EPs, singles, compilations, classical by composer) as rows,
  covers or a table, with one filter for everything: genre, year, tempo, mood, length, what you played
  ([0061](docs/decisions/0061-filters-are-unsaved-crates.md),
  [0082](docs/decisions/0082-classical-and-other-versions.md)).
- **Crates** are playlists that fill themselves from rules, shown as records or as tracks; an artist can be left out
  of one crate or of everything ([0024](docs/decisions/0024-crates.md),
  [0083](docs/decisions/0083-excluding-an-artist.md)).
- **Incoming**: new music waits in a folder of its own until you have heard it. Keep moves an album into your library,
  named as the library names albums; Delete moves it to a trash you can undo from
  ([0013](docs/decisions/0013-keep-and-replace.md), [0034](docs/decisions/0034-one-library-page-and-moves.md)).
- **Health** says what needs you: missing tracks, unreadable files, a server that can't be reached
  ([0063](docs/decisions/0063-one-health-report.md)). A missing track can be fetched on its own, through slskd or from
  a file on the server, and is checked before it is added: the right recording (AcoustID), the right length, truly
  lossless, no clicks ([0080](docs/decisions/0080-single-track-intake.md)).

**Find more in what you have**
- **Home**: music for the moment (the time of day and, with Home Assistant, the weather), new albums, what you used to
  play, artists to explore.
- **Radio** after each track and **Autoplay** when the queue ends, chosen from your own library
  ([0036](docs/decisions/0036-radio.md)).
- **Discover**: new records from the artists you play, records like what you play, what is missing from your artists,
  and new artists, from MusicBrainz and Discogs, asked for through Lidarr where you allow it
  ([0069](docs/decisions/0069-discover.md), [0070](docs/decisions/0070-requests.md)). Your Spotify library, if you
  bring it, shows what of it you already have ([0073](docs/decisions/0073-spotify-library.md)).
- **Search** finds everywhere at once: the library, what Cratarr knows of elsewhere, and MusicBrainz
  ([0076](docs/decisions/0076-search-three-universes.md)).
- With [AudioMuse-AI](https://github.com/NeptuneHub/AudioMuse-AI)'s analysis (optional, separate software; see
  [Optional analysis](#optional-analysis)), mood, energy, danceability and style for every track, and similarity that
  knows how things sound ([0054](docs/decisions/0054-audiomuse-import.md)).
- **Insights**: your collection, your listening, and a map of your playlists
  ([0064](docs/decisions/0064-one-listening-history.md), [0065](docs/decisions/0065-insights-collection-listening-map.md));
  scrobbling to ListenBrainz or Last.fm ([0068](docs/decisions/0068-one-way-in-for-listening.md)).

**Run it**
- Several accounts, each with their own playlists, favourites and history; sign-in on every request, API keys for
  Home Assistant, and optionally an LDAP directory ([0051](docs/decisions/0051-several-users.md),
  [0043](docs/decisions/0043-sign-in-everywhere.md)). A device you share with the others is theirs to use at home only.
- One container: a single static binary with the web app built in, and a small memory budget.
- Diagnostics: what happened and why, across the server and its players, kept for 14 days
  ([0045](docs/decisions/0045-diagnostics.md)).
- Optionally Jellyfin as a second source, and Lidarr and slskd for getting music
  ([0005](docs/decisions/0005-pluggable-sources.md)).

## Running it for good

With Docker Compose:

```yaml
services:
  cratarr:
    image: cratarr                 # built as in Try it; ghcr.io/ozdeboz/cratarr:latest once releases are published
    user: "1000:1000"              # the user and group that own your music files
    ports:
      - "8080:8080"                # the web app, Cratarr's API and OpenSubsonic (/rest)
      - "1704:1704"                # playing in step, for players on your network
    volumes:
      - /srv/music:/music          # writable: the folder that holds your music folders (Music, Incoming, ...)
      - /srv/cratarr:/data         # settings and databases: back it up
    mem_limit: 768m
    restart: unless-stopped
```

- **One writable mount for your music.** Cratarr writes playlists and their backups into your music folder, and moves
  albums between Incoming and Music. When those folders share one mount, a move is a rename: instant, with nothing
  copied ([0010](docs/decisions/0010-writable-libraries.md)). When their parent also holds other media, mount the parent
  read-only and each music folder over it writable (`/srv/media:/media:ro`, then `/srv/media/Music:/media/Music`); a
  move then copies the album first.
- **The same owner as your music**: the container's `user` owns what Cratarr writes. No `PUID` or `UMASK` is needed.
- **Memory**: the process uses tens of MB, but a container's limit also counts the cache of the files it reads, its
  catalogue among them (about 6 KB per track). About 768 MB suits 30,000 tracks.
- **From outside your home**: put it behind a reverse proxy with HTTPS (which also makes the web app installable).
  Every request needs a sign-in already; a proxy can ask Cratarr to check sign-ins for it (`/api/auth/check`, for
  Caddy's `forward_auth`, Traefik's `forwardAuth` or nginx's `auth_request`). Cratarr believes the client's address that
  a proxy forwards (`X-Forwarded-For`) only from a proxy on a local address (the same computer, its Docker network or
  your home network); a proxy anywhere else is named in Settings → Authentication → Trusted proxies (or `CRATARR_PROXIES`).
  What counts as your home network (for shared devices and the guest page) is the local addresses, and any named in
  Settings → Authentication → Home networks.
- **The data folder** holds `settings.json` (secrets: readable by its owner only) and SQLite databases.
  `library.sqlite` is rebuilt when it is missing; the others are worth backing up.

### Optional analysis

Cratarr analyses every track itself: loudness, tempo, waveform, and what the visuals move to. Two outside projects add
more. Neither is part of Cratarr or needed to run it. Each runs as its own software, usually on a stronger computer,
and its results are handed to Cratarr as files in the data folder. What they add is missing without them; everything
else works as usual.

| | Adds | Without it | How it runs |
|---|---|---|---|
| [AudioMuse-AI](https://github.com/NeptuneHub/AudioMuse-AI) | Mood, energy, danceability, style and key per track; similarity by sound for radio, crates and Home | Those filters and sections are empty; radio and similarity go by tags, tempo and listening | AudioMuse-AI analyses the library on its own; `tools/audiomuse_export.py` writes its results to `<data>/audiomuse/` ([0054](docs/decisions/0054-audiomuse-import.md)) |
| [Beat This!](https://github.com/CPJKU/beat_this) | Beats and downbeats as the model hears them, for the visuals | The visuals keep the beat grid of Cratarr's own analysis | `tools/beat_this_pass.py` hears the files Cratarr lists in `<data>/beats/wanted.jsonl` and returns them to `<data>/beats/in/`; on a server with memory to spare (about 650 MB while it hears), `CRATARR_BEAT_THIS=on` lets Cratarr hear new music itself ([0049](docs/decisions/0049-pulse.md)) |

### Configuration

Almost everything is set in Settings. The environment covers what has to be known before the page opens, and seeds a
few settings Settings can change later (what Settings saves wins):

| Variable | Default | |
|---|---|---|
| `CRATARR_BIND` | `0.0.0.0:8080` | The address and port the server listens on. |
| `CRATARR_DATA_DIR` | `/data` in the image | Settings and databases. Without it nothing is kept between starts. |
| `CRATARR_WEB_DIR` | `/web` in the image | The built web app (`web/dist`). Unset outside the image: `../web/dist` beside the source. |
| `CRATARR_SNAPCAST_BIND` | `0.0.0.0:1704` | The port for playing in step; empty turns it off. |
| `CRATARR_SNAPCAST_HOST` | none | The local address players reach that port on (and a Kodi box reaches Cratarr on), when it is not the one they open Cratarr's pages at. Also in Settings → Devices. |
| `CRATARR_BEAT_THIS` | off | `on`: the server hears new music's beats with the Beat this! model itself (about 650 MB while it runs). Off, they come from `tools/beat_this_pass.py` on another computer. Also in Settings → Libraries. |
| `CRATARR_ANALYSIS` | `keep` | The server's own analysis of the music (waveforms, tempo, loudness, the visuals' signals): `keep` analyses new music once half the library has an analysis (a first pass elsewhere, or all of it here), `all` analyses everything here, `off` none. Also in Settings → Libraries. |
| `CRATARR_PROXIES` | local addresses | The proxies whose forwarded client address is believed, as addresses or ranges, comma-separated (`172.19.0.0/16, 192.168.1.5`). Unset: a proxy on a local address (loopback, private ranges, link-local, IPv6 unique local); one elsewhere must be named. Also in Settings → Authentication. |
| `CRATARR_PLAYLIST_DIR` | none | The playlists' folder until a music folder is chosen in Settings. |
| `CRATARR_SUBSONIC_USER`, `CRATARR_SUBSONIC_PASSWORD` | none | An account made (or its password set) at every start. |
| `JELLYFIN_URL`, `JELLYFIN_API_KEY`, `CRATARR_PLAYER_CLIENT` | none | A Jellyfin source on the first start, until one is saved in Settings. |
| `LIDARR_URL`, `LIDARR_API_KEY` | none | Lidarr on the first start, until it is saved in Settings. |
| `RUST_LOG` | `info` | What the server logs. Settings → Diagnostics can log more for a while. |

### Apps

- **Web**: every page of Cratarr. It plays by itself, and over HTTPS it installs as an app.
- **Android**: [android/](android), a thin app around Cratarr's pages that plays the music itself (built from its folder;
  its APK will come with each release). It also plays the phone's own music (Local mode, [0052](docs/decisions/0052-local-library.md)).
- **Desktop**: [desktop/](desktop), Cratarr's pages in a window of their own, for Linux (an APT repository once releases
  are published). A Windows installer is prepared but not yet built or tried.
- **Subsonic apps**: point them at Cratarr's address and sign in with your Cratarr account.
- **Kodi**: added in Settings → Players, it becomes one of Cratarr's players over Kodi's own remote control ("Allow
  remote control via HTTP"), with nothing installed on Kodi.

## Make it your own

Cratarr is meant to be changed. Every choice that shapes it is written down with its reasons, so a change can start
from why things are as they are rather than from guessing:

- [docs/decisions](docs/decisions): one decision per file, with dated amendments as it changed. "The owner" in them is
  the person who decides what Cratarr does; quotes are theirs.
- [docs/architecture.md](docs/architecture.md): where things live in the code and how the parts talk.
- [docs/ui-conventions.md](docs/ui-conventions.md): how pages look and behave.
- [docs/plans.md](docs/plans.md): bigger ideas, not decided yet.
- [CONTRIBUTING.md](CONTRIBUTING.md): building it, running it against your own music, and checking a change.

## Design goals

- Lean: one container, one static binary, a small memory budget.
- Your data stays yours: playlists in plain files beside the music, and nothing sent anywhere but your own server.
- Dependable after months untouched: few dependencies, pinned builds, tests around the parts that matter.

## Stack

- Server: Rust ([axum](https://github.com/tokio-rs/axum), SQLite), one static binary in an otherwise empty image.
- Web app: Svelte 5 and Vite, built to static files that the server serves from a folder beside it (`/web` in the image).
- Android app: Kotlin and Media3, grown from Jellyfin's Android app.
- Desktop app: Electron around the web app.

## Credits

Cratarr borrows freely from projects that got things right:

- [Jellyfin](https://jellyfin.org), the media server Cratarr was first built alongside, and whose Android app
  Cratarr's grew from.
- [Navidrome](https://www.navidrome.org), [gonic](https://github.com/sentriz/gonic) and
  [nghe](https://github.com/vnghia/nghe), whose ways of serving OpenSubsonic Cratarr's follow
  ([0030](docs/decisions/0030-cratarr-serves-opensubsonic.md)).
- [Symfonium](https://symfonium.app) and [Finamp](https://github.com/finamp-app/finamp), whose player, menus and
  settings Cratarr's follow ([0019](docs/decisions/0019-remote-features-from-finamps-player.md),
  [0021](docs/decisions/0021-menus-are-finamp-style-sheets.md)).
- [Snapcast](https://github.com/badaix/snapcast), whose protocol and timekeeping playing in step uses
  ([0037](docs/decisions/0037-sync-play.md)).
- [Seerr](https://github.com/seerr-team/seerr), for its layout and its settings that choose from what the servers
  already know ([0009](docs/decisions/0009-library-uses.md)).
- [AudioMuse-AI](https://github.com/NeptuneHub/AudioMuse-AI) for its analysis, [Beat This!](https://github.com/CPJKU/beat_this)
  for the beats, and [ListenBrainz](https://listenbrainz.org) for listening history, imported and scrobbled.
- The photographers of Home's skies and the flows' backgrounds, on Unsplash (named in
  [web/public/skies/CREDITS.txt](web/public/skies/CREDITS.txt) and [web/public/backdrops/CREDITS.txt](web/public/backdrops/CREDITS.txt)).
- [libopus](https://opus-codec.org) (BSD) for the Opus copies, [LAME](https://lame.sourceforge.io) (LGPL) for the MP3
  copies a browser that takes neither FLAC nor Opus plays, [alac-encoder](https://crates.io/crates/alac-encoder) (Apache
  2.0 or MIT) for the ALAC copies, [Symphonia](https://github.com/pdeljanov/Symphonia) (MPL 2.0) to read the files, and
  [rusty-chromaprint](https://github.com/darksv/rusty-chromaprint) (MIT) with [AcoustID](https://acoustid.org) to know a
  recording by its sound.

## License

Cratarr is free software under the GNU Affero General Public License, version 3 or (at your option) any later version:
see [LICENSE](LICENSE). Whoever runs a modified Cratarr for others over a network offers them its source
([0029](docs/decisions/0029-agpl-licence.md)).

The Android app in [android/](android) is [Jellyfin's Android app](https://github.com/jellyfin/jellyfin-android) made
into Cratarr's, and keeps its own licence, the GNU General Public License version 2 ([android/LICENSE.md](android/LICENSE.md)).
