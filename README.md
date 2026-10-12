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

## Configuration

The container: one mount that holds your music libraries, such as their parent folder (read only is enough), with
the Music library's folder mounted writable inside it, since the playlists are written there (Settings says which
library it can't see), and optionally `CRATARR_BIND`, `CRATARR_DATA_DIR` and `CRATARR_PLAYLIST_DIR` (the Music
folder until a Music library is saved; see `server/src/config.rs`).

Sources and library uses are set up on the Settings page and saved in `CRATARR_DATA_DIR/settings.json` (owner-only: it holds secrets).
Until something is saved there, one Jellyfin source comes from `JELLYFIN_URL`, `JELLYFIN_API_KEY` and the
optional `CRATARR_PLAYER_CLIENT`. Secrets are never sent back to the page, and a saved
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

## License

MIT
