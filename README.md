# Cratarr

A small, self-hosted web app for keeping your music playlists in order. It works alongside your
player (starting with [Jellyfin](https://jellyfin.org)) instead of replacing it.

**Status: early, working prototype.**

## The idea

Players are good at playing music and weak at managing playlists. Cratarr sits beside them:

- see every playlist at once, with the ones that contain the track you are playing highlighted
- add or remove the playing track with one tap
- later: keep playlists healthy when library files are upgraded, renamed or removed

It is deliberately small.

## How it works

- Playlists are plain `.m3u` files in your Music folder (the format Jellyfin imports). Cratarr reads and edits the
  files directly, with a backup before every change.
- Sources tell Cratarr what is playing; after an edit Cratarr tells every source, so each re-reads the playlist
  (Jellyfin: `FullRefresh` with `ReplaceAllMetadata`, the only mode that handles both added and removed entries).
- Each server library is **Music** (the playlists and the tracks that can go in them), **Incoming** (new music
  waiting to be heard, never added to playlists) or not used, set in Settings → Libraries. Cratarr finds the
  folder behind each library and says when it can't see a library's files; see
  [decision 0009](docs/decisions/0009-library-uses.md).

## Sources

A source is a server Cratarr works alongside; Jellyfin is the first. Each kind is a plugin compiled in: a module
that implements the `Source` trait in [`server/src/source/mod.rs`](server/src/source/mod.rs) (which explains how to
write one) and is listed in `KINDS`. A plugin declares its settings fields, so the Settings page draws its form
with no plugin-specific UI code.

- Several sources can be on at once, for example two servers reading the same music folder. A source picker then
  appears in the sidebar (or above the page on phones). Each browser picks a source, or "Automatic": whichever
  started playing most recently.
- Cratarr finds the folder behind each library by itself (`placement.rs`). It strips leading folders off sample
  track paths until the rest is a file in one of its mounted folders. Sources need not agree on paths: Jellyfin,
  for one, reports `/data/media/Music/...` for what Cratarr sees as `/music/...`.
- With Jellyfin, the page follows a chosen player ("Finamp on Lenovo Tab", "Finamp, any device" or any player).
- Playlists are the same for everyone. Jellyfin shows `.m3u` playlists to every user with access to the library,
  and its per-user ownership and sharing do not apply to them, so Cratarr does not ask for a Jellyfin user (it uses
  an administrator internally for the one call that needs one).
  Per-user playlists belong with a future integration of Jellyfin's own playlists.

See [decision 0005](docs/decisions/0005-pluggable-sources.md).

## Catalogue

Cratarr keeps its own index of every audio file of the Music library and of the Incoming libraries it can see: path, size, tags as stored, and each server's id for it. It fills itself in the background (a first
fill of about 24,000 files takes some 12 minutes) and then follows changes every few minutes. It is derived data
in `CRATARR_DATA_DIR/catalogue.sqlite`; deleting it only costs a refill. Tags are split on read by editable rules,
never rewritten in the files, and files that cannot be read properly are listed on the Repairs page. See
[decision 0008](docs/decisions/0008-catalogue.md).

## Album cover

The page header shows the cover of the playing track's album, small, next to the title. `GET /api/cover/{source}/{item_id}`
looks the track up in that source, maps it to its library entry and serves the first of `cover.jpg`, `cover.jpeg`,
`cover.png`, `folder.jpg`, `folder.jpeg`, `folder.png`, `front.jpg`, `front.png` (any letter case) found in the
album folder, read from the Music folder on disk. It answers 404 when there is none, when the
track is unknown or outside the library, or when the file is over 10 MB. Responses carry an `ETag` and
`Cache-Control: public, max-age=86400`, and honour `If-None-Match`. Images are not resized or cached in memory.
Security rules are in [decision 0003](docs/decisions/0003-album-cover-endpoint.md).

## Progress bar

Under the playing track's title. `now_playing.progress` in `/api/state` carries the position, length and pause
state, taken from the Jellyfin session that is playing that exact track (the chosen player first). When no
session reports it (a stale session record, see `nowplaying.rs`), the position is estimated from when the activity
log says the track started, and the bar is shown faded with a `≈`. The page advances the bar on its own clock
between polls and only re-syncs to the server on a track change, a pause or resume, or a gap of more than 1.5 s
(a seek).

## Design goals

- Lean: one container, a single static binary, a small memory budget.
- Loosely coupled: talks to Jellyfin through its public API.
- Dependable after months untouched: few dependencies, pinned builds, tests around the parts that matter.

## Standard setup

One container, the music read and written as the library's own owner, its data on a volume of its own:

```yaml
services:
  cratarr:
    image: cratarr:0.6.2           # pin the tag; update by changing it
    user: "1000:1000"              # the user and group that own the music files (the *arr apps' PUID:PGID)
    ports: ["8080:8080"]
    environment:
      CRATARR_DATA_DIR: /data
    volumes:
      - /srv/music:/music          # writable: the folder holding the music libraries (Music, Incoming...)
      - /srv/cratarr:/data         # settings, catalogue, journal; back it up
    mem_limit: 256m
    restart: unless-stopped
```

- **One writable mount for the music libraries**, their parent folder, so Keep moves an album from Incoming into
  Music by renaming it (instant, nothing copied) and Delete moves it into that folder's trash
  ([decision 0010](docs/decisions/0010-writable-libraries.md)). Point the media server and Lidarr at the same
  folders; Cratarr finds each library inside the mount by the paths the server reports.
- **When that parent also holds other media** (films, series, downloads), don't give Cratarr all of it: mount the
  parent read-only and each music library over it writable (`/srv/media:/media:ro`, then `/srv/media/Music:/media/Music`
  and `/srv/media/incoming:/media/incoming`). Everything works the same, except that Keep then crosses two mounts
  and copies the album before removing the original (seconds per album).
- **The same owner as the music**: the container's `user` is the files' owner and group, so what Cratarr writes
  (playlists, backups, moved albums) keeps the library's ownership; no `PUID` or `UMASK` is needed.
- **Memory**: the process itself stays small (tens of MB), but a container's memory limit also counts the cache of
  the files it reads, its catalogue database among them (about 7 KB per track, ~200 MB for 30,000 tracks). Give it
  at least that much (256 MB for a library of that size): with less, every Crates request reads the catalogue from
  disk again, and starting can be killed for lack of memory.
- **No login**: keep it on your LAN or VPN, or behind a reverse proxy that authenticates; the proxy's HTTPS also
  makes the page installable as an app.
- Settings (sources, libraries, Lidarr) are made on the Settings page and kept in `/data/settings.json`, readable by
  its owner only.

## Configuration

The container: one writable mount that holds your music libraries, such as their parent folder (Settings says which
library it can't see or can't write to; Cratarr only writes playlists in the Music folder and deleted albums into
the trash of an Incoming folder, see decision [0010](docs/decisions/0010-writable-libraries.md)), and optionally `CRATARR_BIND`, `CRATARR_DATA_DIR` and `CRATARR_PLAYLIST_DIR` (the Music
folder until a Music library is saved; see `server/src/config.rs`).

Each playlist is one `.m3u` file in the playlist folder, `Music/.playlists`. Per playlist, "Sync to…" in its menu keeps
copies of it on targets: Shared (a copy in Music the media server imports for everyone) and accounts' own Jellyfin
playlists (kept in step both ways); see [decision 0014](docs/decisions/0014-sync-links.md). Playlists that were in Music
are adopted into the playlist folder once, each linked Shared, so nothing visible changes. A voice assistant finds a
playlist by a spoken name and gets its tracks to play ([decision 0015](docs/decisions/0015-playlists-by-spoken-name.md)).

Sources and library uses are set up on the Settings page and saved in `CRATARR_DATA_DIR/settings.json` (owner-only: it holds secrets).
Until something is saved there, one Jellyfin source comes from `JELLYFIN_URL`, `JELLYFIN_API_KEY` and the
optional `CRATARR_PLAYER_CLIENT`. Lidarr (optional: Delete in Incoming asks it to stop fetching the album) is set
there too, under Connections; `LIDARR_URL` and `LIDARR_API_KEY` are its first-run default. Secrets are never sent back to the page, and a saved
secret is only reused while the fields it is tied to (Jellyfin: the address) stay the same. `CRATARR_MUSIC_PREFIXES`,
`CRATARR_JELLYFIN_PLAYLIST_DIR` and `JELLYFIN_USER_ID` are no longer used. See decisions [0004](docs/decisions/0004-settings-page.md)
and [0005](docs/decisions/0005-pluggable-sources.md).

A source that is down does not stop Cratarr: its status shows on the Settings page, and its libraries are looked for
again every minute. An unreadable Music folder does stop it.

## Navigation

A sidebar on screens 1024 px and wider; below that, a bar at the bottom of the screen (with a "More" menu once
there are more than five pages). With more than one source, a source picker sits under the connection line in the
sidebar, or above the page on phones.

## Stack

- Core: Rust ([axum](https://github.com/tokio-rs/axum))
- UI: Svelte 5 + Vite, built to static files and embedded in the binary

See [docs/decisions](docs/decisions) for why, and what is planned next.

## Credits

- [Jellyfin](https://jellyfin.org), the media server Cratarr was first built alongside.
- [Finamp](https://github.com/finamp-app/finamp), whose player and menus Cratarr's follow ([0019](docs/decisions/0019-remote-features-from-finamps-player.md), [0021](docs/decisions/0021-menus-are-finamp-style-sheets.md)).
- [Seerr](https://github.com/seerr-team/seerr), for its layout, its settings that choose from what the servers already know ([0009](docs/decisions/0009-library-uses.md)), and sign-in through Jellyfin ([0015](docs/decisions/0015-playlists-by-spoken-name.md)).

## License

Cratarr is free software under the GNU Affero General Public License, version 3 or (at your option) any later version: see [LICENSE](LICENSE). Whoever runs a modified Cratarr for others over a network offers them its source ([decision 0029](docs/decisions/0029-agpl-licence.md)).

The Android app in [android/](android) is [Jellyfin's Android app](https://github.com/jellyfin/jellyfin-android) made into Cratarr's, and keeps its own licence, the GNU General Public License version 2 ([android/LICENSE.md](android/LICENSE.md)).
