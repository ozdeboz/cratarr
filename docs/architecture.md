# How Cratarr is put together

A map for finding your way in the code. The reasons behind each part are in [decisions](decisions); this says where
the parts are and how they talk.

## The pieces

```
  web app (Svelte)  ──┐                       ┌── your music folders (.m3u playlists beside the music)
  Android app ────────┼── HTTP and SSE ─────> │  server (Rust, one binary) ── SQLite in the data folder
  Subsonic apps ──────┘     /api  /rest       └── optional: Jellyfin, Lidarr, Home Assistant, AudioMuse-AI
  Kodi, snapclient  <── Cratarr drives them (Kodi's remote control, Snapcast's protocol)
```

- **The server** is one static binary. It reads the music folders, keeps the catalogue and every user's data in SQLite,
  serves the web app (built into the binary), Cratarr's own API (`/api`) and OpenSubsonic (`/rest`), and holds every
  player's queue.
- **The web app** is every page. It plays by itself in a browser, and in the Android app it is the app's whole face.
- **The Android app** is a thin shell around the web app: it plays the music with Android's own player (Media3),
  keeps downloads, and talks to the page over a message port.

## Server (`server/`)

- `src/main.rs`: start-up, the environment, the background passes.
- `src/api.rs`: Cratarr's HTTP API (axum); handlers call the services in `app/`.
- `src/app/`: the features, one module each: `playlists`, `library`, `incoming`, `queue` and `playon` (the players and
  their queues), `radio` and `similar` (what plays next), `moment` (Home's music for the moment), `health`,
  `history` and `taste` (listening), `insights`, `visuals`, `kodi`, `listenbrainz`, `audiomuse` (its analysis taken in).
- `src/serve/`: Cratarr as an OpenSubsonic server, and sign-in for everything: sessions, accounts, API keys, LDAP.
- `src/catalogue/`: every audio file and its tags, read once and kept up to date.
- `src/source/`: where music comes from (Cratarr's own folders; Jellyfin optionally), behind one trait.
- `src/crates.rs`: crates, the rules that make playlists by themselves; filters use the same engine.
- `src/similarity.rs`: what makes music alike, for radio, Autoplay and "more like this".
- `src/analysis/`: waveform, tempo and loudness, worked out once per track.
- `src/journal.rs`: every change is written down first, so it can be undone.
- `src/snapcast/`: playing in step, as a Snapcast server.
- `src/userdata/`: what each user did: plays, stars, ratings, history.
- `rules/`: what plays and in what order, one crate shared with players that play on their own.
- Tests sit beside the code (`#[cfg(test)]`) and in `tests/`; `cargo test` runs them all.

## Web app (`web/`)

- `src/App.svelte`: the shell: navigation, the page, the panel beside it, the player.
- `src/*.svelte`: the pages (Home, Library, Crates, Playlists, Downloads, Insights, Health, Settings, Local mode's).
- `src/lib/api.ts`: every call to the server, typed.
- `src/lib/localPlayer.svelte.ts`: the player: it plays in the page, or hands the queue to the app or another player.
- `src/lib/playOn.svelte.ts`: the other players, and which one this page drives.
- `src/lib/panel.svelte.ts`: what the panel beside the page shows, kept in the address.
- `src/lib/app.svelte.ts`: the message port to the Android app (`window.CratarrApp`).
- `src/lib/settings/`: Settings' parts, one file each.
- `src/lib/visuals/`: the visuals' engine and its modules.
- `tests/`: `pnpm test`; `pnpm check` for types and Svelte.

## Android app (`android/`)

- `MainActivity.kt`: the WebView with the page, and the message port to it.
- `PlayerService.kt` and `PagePlayer.kt`: the player, its media session, and what the page asks of it.
- `PlayOnLink.kt`: the app as one of Cratarr's players, reachable while the screen is off.
- `Downloads.kt` and `Kept.kt`: Download once and Sync to this device.
- `Radio.kt`: Radio and Autoplay, played by the phone itself, offline too.
- `DeviceMusic.kt`: Local mode, the phone's own music.

## Running it while you change it

- The web app's dev server (`pnpm dev` in `web/`) sends `/api` to a running server (`CRATARR_URL`, by default
  `http://127.0.0.1:8080`), so pages change as you save them.
- The server (`cargo run` in `server/`) with `CRATARR_DATA_DIR` set keeps its settings between starts; choose your
  music folder in Settings → Libraries.
- [CONTRIBUTING.md](../CONTRIBUTING.md) has the checks a change passes.
