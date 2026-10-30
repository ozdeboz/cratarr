# How Cratarr is put together

A map for finding your way in the code. The reasons behind each part are in [decisions](decisions); this says where
the parts are and how they talk.

## The pieces

```
  web app (Svelte)  ──┐                       ┌── your music folders (.m3u playlists beside the music)
  Android app ────────┤                       │
  desktop app ────────┼── HTTP and SSE ─────> │  server (Rust, one binary) ── SQLite in the data folder
  Subsonic apps ──────┘     /api  /rest       └── optional: Jellyfin, Lidarr, slskd, Home Assistant, AudioMuse-AI
  Kodi, snapclient  <── Cratarr drives them (Kodi's remote control, Snapcast's protocol)
```

- **The server** is one static binary. It reads the music folders, keeps the catalogue and every user's data in SQLite,
  serves the web app (built into the binary), Cratarr's own API (`/api`) and OpenSubsonic (`/rest`), and holds every
  player's queue.
- **The web app** is every page. It plays by itself in a browser, and in the Android and desktop apps it is the app's
  whole face.
- **The Android app** is a thin shell around the web app: it plays the music with Android's own player (Media3),
  keeps downloads, and talks to the page over a message port.
- **The desktop app** ([desktop/](../desktop)) shows the web app in a window of its own (Electron), starts at login and
  keeps playing with the window closed; the page plays the music, as in a browser.

## One source for what the server, the page and the app share

- **The protocol**: the Play On messages (what a player is asked, what it reports, a player's view, a queue's tracks)
  are the server's Rust types. `cargo test` writes them out as TypeScript into `web/src/lib/generated/` (ts-rs), and
  the app's contract test reads the same files, so a field changed in Rust shows up as a difference everywhere.
- **The rules**: `shared/rules.json` holds the players' rules in one table: Previous's restart, the wind's limits, the
  tape's ranges, the brake and its worked cases. The server's tests, the app's and the page's
  (`web/tests/rules-table.test.ts`) each check their own code against it.
- **The rules crate** (`server/rules/`): what plays and in what order, and the brake, one implementation for the
  server and for players that play on their own. It also builds to WebAssembly (`web/src/lib/wasm/`), which the page
  can use instead of its own code behind a switch (off by default).

## Server (`server/`)

- `src/main.rs`: start-up, the environment, the background passes.
- `src/api.rs`: Cratarr's HTTP API (axum); handlers call the services in `app/`.
- `src/app/`: the features, one module each, among them:
  - `playlists`, `library`, `incoming`, `albums`, `collection`, `filter` (the one filter engine);
  - `queue` and `playon` (the players and their queues; `playon/along.rs`: following another listener),
    `queue_requests` (tracks asked of a leader), `radio`, `similar` (what plays next);
  - `home` (Home's shelves, New for you among them), `moment`, `discover`, `requests` (albums asked for), `search`;
  - `classical` and `works` (the Classical kind, a work's other versions, from MusicBrainz), `excluded_artists`;
  - `track_fetch` (a single missing track, from Soulseek or a file on the server, checked before it is added);
  - `health`, `history` and `taste` (listening), `insights`, `visuals`, `kodi`, `listenbrainz`, `audiomuse`;
  - `runtime` (the server's own settings: trusted proxies, home networks, the guest page).
- `src/serve/`: Cratarr as an OpenSubsonic server, and sign-in for everything: sessions, accounts, API keys, LDAP;
  `guest.rs` (the guest page, at home only); `access.rs` and `proxy.rs` (who a request is, and from where).
- `src/catalogue/`: every audio file and its tags, read once and kept up to date.
- `src/source/`: where music comes from (Cratarr's own folders; Jellyfin optionally), behind one trait.
- `src/crates.rs`: crates, the rules that make playlists by themselves; filters use the same engine.
- `src/similarity.rs`: what makes music alike, for radio, Autoplay and "more like this".
- `src/analysis/`: waveform, tempo and loudness, worked out once per track.
- `src/journal.rs`: every change is written down first, so it can be undone.
- `src/snapcast/`: playing in step, as a Snapcast server.
- `src/userdata/`: what each user did: plays, stars, ratings, history.
- Tests sit beside the code (`#[cfg(test)]`) and in `tests/`; `cargo test --workspace` runs them all, the router's
  sweep over every route among them.

## Web app (`web/`)

- `src/App.svelte`: the shell: navigation, the page, the panel beside it, the player.
- `src/*.svelte`: the pages (Home, Crates, Playlists, Library, Discover, Downloads, Insights, Health, Settings, Local
  mode's); `src/guest/`: the guest page, its own entry (`guest.html`).
- `src/lib/api.ts`: every call to the server, typed; the Play On types are the generated ones.
- The player:
  - `src/lib/player/port.svelte.ts`: the one place that decides which player the page drives (this page's own, or the
    server's for the app, a Play On player or Kodi), and what each side can do;
  - `src/lib/player/clock.ts`: where another player's track is now, from its reports;
  - `src/lib/player/tape.ts` and the slots beside it: the tape (winding, holding, braking) as one state, its passing
    states with their deadlines (`graces.ts`);
  - `src/lib/localPlayer.svelte.ts`: this page's own player;
  - `src/lib/playOn.svelte.ts`: the other players, and which one this page drives (`output.svelte.ts`: moving the
    music between them, the screen following the music when it moves).
- `src/lib/navStep.svelte.ts`: going somewhere as one step: what a tap leaves (the search, the full player, a panel)
  goes when the place it goes to shows, and back restores it (`searchKept.ts`).
- `src/lib/panel.svelte.ts`: what the panel beside the page shows, kept in the address.
- `src/lib/menuTable.ts`: every menu's entries, their groups and their order, as one table (`MenuTable.svelte` draws
  it); each menu decides only what shows.
- `src/lib/deviceSettings.ts`: this device's settings, each with one owner (the app, the browser or the server), and
  `device.svelte.ts`, the one way to read and change the ones both kinds of device have.
- `src/lib/app.svelte.ts`: the message port to the Android app (`window.CratarrApp`).
- `src/lib/settings/`: Settings' parts, one file each.
- `src/lib/visuals/`: the visuals' engine and its modules: one engine, each module's own values handed to the renderer,
  one macro resolver (`macro-map.ts`), one way to change a rack (`applyEdit` in `patch-edits.ts`).
- `tests/`: `pnpm test`; `pnpm check` for types and Svelte. The visuals' golden frames (`tests/golden/`,
  `tests/harness/golden.ts`) record what the standard rack and the library's racks give, frame by frame, so a change
  that alters the picture shows.

## Android app (`android/`)

- `MainActivity.kt`: the WebView with the page, and the message port to it.
- `PlayerService.kt` and `PagePlayer.kt`: the player, its media session, and what the page asks of it (as the server's
  Play On commands).
- `PlayOnLink.kt`: the app as one of Cratarr's players, reachable while the screen is off.
- `Downloads.kt` and `Kept.kt`: Download… to this device and Sync to this device.
- `Radio.kt`: Radio and Autoplay, played by the phone itself, offline too.
- `DeviceMusic.kt`: Local mode, the phone's own music.

## Running it while you change it

- The web app's dev server (`pnpm dev` in `web/`) sends `/api` to a running server (`CRATARR_URL`, by default
  `http://127.0.0.1:8080`), so pages change as you save them.
- The server (`cargo run` in `server/`) with `CRATARR_DATA_DIR` set keeps its settings between starts; choose your
  music folder in Settings → Libraries.
- [CONTRIBUTING.md](../CONTRIBUTING.md) has the checks a change passes.
