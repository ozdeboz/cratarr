# Cratarr

A self-hosted music server for your own library, built around playlists. Cratarr reads the music folders you give it
and plays them in its web app, its Android app and any OpenSubsonic app. From any one of them it plays on the others
(Play On). Your playlists stay plain `.m3u` files beside your music.

**Status:** in active development and used every day on one home server. There is no release or published image
yet: you build it from source (below).

## What it does

- **Your library**, from the folders you choose: albums, artists and tracks, with covers from the album folders and
  every track's waveform, tempo and loudness worked out once on the server
  ([0032](docs/decisions/0032-cratarr-is-its-own-source.md), [0044](docs/decisions/0044-folders.md),
  [0035](docs/decisions/0035-audio-analysis.md)).
- **Playlists are `.m3u` files** in your Music folder, so any player can read them. A backup is made before every
  change, and every change can be undone ([0002](docs/decisions/0002-playlists-are-m3u-files.md),
  [0026](docs/decisions/0026-one-undo-journal.md)).
- **Crates** are playlists that make themselves: your favourites, the tracks in no playlist yet, and your own
  cross-sections of the library ([0024](docs/decisions/0024-crates.md)).
- **Incoming**: new music waits in a folder of its own until you have heard it. Keep moves an album into Music, named
  as Music names albums. Delete moves it to a trash you can undo from, and tells Lidarr (when connected) to stop fetching it
  ([0013](docs/decisions/0013-keep-and-replace.md), [0034](docs/decisions/0034-one-library-page-and-moves.md)).
- **Players**: the page plays by itself. With Play On, any of Cratarr's players (a browser, the Android app, a Kodi box)
  is commanded from any other, and Cratarr holds every queue, so music moves from one to another where it was
  ([0031](docs/decisions/0031-android-app-and-play-on.md), [0033](docs/decisions/0033-cratarr-holds-the-queues.md)).
  Several players can play one queue in step ([0037](docs/decisions/0037-sync-play.md)), and radio keeps the music
  going from your own library ([0036](docs/decisions/0036-radio.md)).
- **OpenSubsonic**: Subsonic and OpenSubsonic apps play from it, with your Cratarr account
  ([0030](docs/decisions/0030-cratarr-serves-opensubsonic.md)).
- **Offline**: the Android app keeps playlists, crates, albums and artists on the device and in step with Cratarr
  ([0038](docs/decisions/0038-kept-on-this-device.md)).
- **Sign-in on every request**, with Cratarr's own sign-in page, API keys for other programs, and optionally an LDAP
  directory ([0039](docs/decisions/0039-sign-in-page-and-sessions.md), [0040](docs/decisions/0040-directory-sign-in.md),
  [0043](docs/decisions/0043-sign-in-everywhere.md)).
- **Home automation**: playlists found by a spoken name for a voice assistant, and every player commanded over
  Cratarr's API ([0015](docs/decisions/0015-playlists-by-spoken-name.md)).
- **Diagnostics**: what happened, with its reasons, across the server and its players, kept for 14 days and shown in
  Settings ([0045](docs/decisions/0045-diagnostics.md)).
- **Jellyfin**, optionally, as a second source of music next to Cratarr's own
  ([0005](docs/decisions/0005-pluggable-sources.md)).

## Install

Cratarr runs as one container: a single static binary with the web app built in.

1. Build the image:

   ```sh
   git clone <this repository> cratarr && cd cratarr
   docker build -t cratarr:local .
   ```

2. Run it, for example with Docker Compose:

   ```yaml
   services:
     cratarr:
       image: cratarr:local
       user: "1000:1000"              # the user and group that own your music files
       ports:
         - "8080:8080"                # the web app, Cratarr's API and OpenSubsonic (/rest)
         - "1704:1704"                # sync play, for players on your network
       environment:
         CRATARR_DATA_DIR: /data
       volumes:
         - /srv/music:/music          # writable: the folder that holds your music folders (Music, Incoming, ...)
         - /srv/cratarr:/data         # settings and databases: back it up
       mem_limit: 768m
       restart: unless-stopped
   ```

3. Open `http://<server>:8080`, make the first account, and in Settings → Libraries add your Music folder (and an
   Incoming folder, if you use one).

Things to know:

- **One writable mount for your music.** Cratarr writes playlists and their backups into Music, and moves albums
  between Incoming and Music. When those folders share one mount, a move is a rename: instant, with nothing copied
  ([0010](docs/decisions/0010-writable-libraries.md)). When their parent also holds other media (films, downloads),
  mount the parent read-only and each music folder over it writable (`/srv/media:/media:ro`, then
  `/srv/media/Music:/media/Music`). A move then copies the album before removing it, which takes seconds per album.
- **The same owner as your music**: the container's `user` is the owner and group of the music files, so whatever
  Cratarr writes keeps the library's ownership. No `PUID` or `UMASK` is needed.
- **Memory**: the process itself uses tens of MB, but a container's memory limit also counts the cache of the files it
  reads, its catalogue database among them (about 6 KB per track). About 768 MB suits a library of 30,000 tracks.
  With much less, every page reads the catalogue from disk again.
- **From outside your home network**: put Cratarr behind a reverse proxy with HTTPS. HTTPS also makes the web app
  installable on phones and computers. Every request needs a sign-in already. A proxy can also ask Cratarr to check
  sign-ins for it (`/api/auth/check`, for Caddy's `forward_auth`, Traefik's `forwardAuth` or nginx's
  `auth_request`).
- **The data folder** holds `settings.json` (readable by its owner only: it holds secrets) and Cratarr's SQLite
  databases. `library.sqlite` is rebuilt from the catalogue when it is missing; the others are worth backing up.

## Configuration

Almost everything is set in Settings. The environment covers what has to be known before the page opens:

| Variable | Default | |
|---|---|---|
| `CRATARR_BIND` | `0.0.0.0:8080` | The address and port the server listens on. |
| `CRATARR_DATA_DIR` | none | Settings and databases. Without it nothing is kept between starts. |
| `CRATARR_SNAPCAST_BIND` | `0.0.0.0:1704` | Sync play's port; set it empty to turn sync play off. |
| `CRATARR_SNAPCAST_HOST` | none | The local address players reach sync play's port on (and a Kodi box reaches Cratarr on), when it is not the one they open Cratarr's pages at (a public name in front, say). |
| `CRATARR_PLAYLIST_DIR` | none | The playlists' folder until a Music folder is chosen in Settings. |
| `CRATARR_SUBSONIC_USER`, `CRATARR_SUBSONIC_PASSWORD` | none | An account made (or its password set) at every start. |
| `JELLYFIN_URL`, `JELLYFIN_API_KEY`, `CRATARR_PLAYER_CLIENT` | none | A Jellyfin source on the first start, until one is saved in Settings. |
| `LIDARR_URL`, `LIDARR_API_KEY` | none | Lidarr on the first start, until it is saved in Settings. |
| `RUST_LOG` | `info` | What the server logs. Settings → Diagnostics can log more for a while. |

## Apps

- **The web app**: every page of Cratarr. It plays by itself, and over HTTPS it installs as an app.
- **Android**: [android/](android) is a thin app around Cratarr's pages. It plays the music itself, stays reachable
  for Play On with the screen locked while Stay ready is on (Settings → This device), and keeps music on the device
  for offline play. It is not released yet: build it from that folder.
- **Subsonic apps**: point them at Cratarr's address and sign in with your Cratarr account.
- **Kodi**: added in Settings → Players, it becomes one of Cratarr's players, driven over Kodi's own remote control
  ("Allow remote control via HTTP"), with nothing installed on Kodi. In a group it follows and never leads.

## Documentation

- [docs/decisions](docs/decisions): how Cratarr works and why, one decision per file. This is the reference.
- [docs/ui-conventions.md](docs/ui-conventions.md): how its pages look and behave.
- [docs/plans.md](docs/plans.md): bigger ideas that are not decided yet.
- [CONTRIBUTING.md](CONTRIBUTING.md): building, testing and proposing a change.

## Design goals

- Lean: one container, one static binary, a small memory budget.
- Your data stays yours: playlists in plain files beside the music, and nothing sent anywhere but your own server.
- Dependable after months untouched: few dependencies, pinned builds, tests around the parts that matter.

## Stack

- Server: Rust ([axum](https://github.com/tokio-rs/axum), SQLite), one static binary in an otherwise empty image.
- Web app: Svelte 5 and Vite, built to static files and embedded in the binary.
- Android app: Kotlin and Media3, grown from Jellyfin's Android app.

## Credits

- [Jellyfin](https://jellyfin.org), the media server Cratarr was first built alongside, and whose Android app
  Cratarr's grew from.
- [Navidrome](https://www.navidrome.org) and [gonic](https://github.com/sentriz/gonic), whose ways of serving
  OpenSubsonic Cratarr's follow ([0030](docs/decisions/0030-cratarr-serves-opensubsonic.md)).
- [Snapcast](https://github.com/badaix/snapcast), whose protocol and timekeeping sync play uses
  ([0037](docs/decisions/0037-sync-play.md)).
- [Finamp](https://github.com/finamp-app/finamp), whose player, menus and settings Cratarr's follow
  ([0019](docs/decisions/0019-remote-features-from-finamps-player.md), [0021](docs/decisions/0021-menus-are-finamp-style-sheets.md),
  [ui-conventions](docs/ui-conventions.md)).
- [Seerr](https://github.com/seerr-team/seerr), for its layout and its settings that choose from what the servers
  already know ([0009](docs/decisions/0009-library-uses.md)).

## License

Cratarr is free software under the GNU Affero General Public License, version 3 or (at your option) any later version:
see [LICENSE](LICENSE). Whoever runs a modified Cratarr for others over a network offers them its source
([decision 0029](docs/decisions/0029-agpl-licence.md)).

The Android app in [android/](android) is [Jellyfin's Android app](https://github.com/jellyfin/jellyfin-android) made
into Cratarr's, and keeps its own licence, the GNU General Public License version 2 ([android/LICENSE.md](android/LICENSE.md)).
