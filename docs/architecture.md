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
  serves the web app (from a folder beside it: `CRATARR_WEB_DIR`, else the image's `/web`), Cratarr's own API (`/api`)
  and OpenSubsonic (`/rest`), and holds every player's queue. `GET /api/capabilities` lists what it can do, so the
  page leaves out what it can't.
- **The web app** is every page. It plays by itself in a browser, and in the Android and desktop apps it is the app's
  whole face.
- **The Android app** is a thin shell around the web app: it plays the music with Android's own player (Media3),
  keeps downloads, and talks to the page over a message port. It carries its own copy of the page, opened when the
  server is out of reach; of the two, the newer is used (each page's `build.json`).
- **The desktop app** ([desktop/](../desktop)) shows the web app in a window of its own (Electron), starts at login and
  keeps playing with the window closed; the page plays the music, as in a browser.

## One source for what the server, the page and the app share

- **The protocol**: the Play On messages (what a player is asked, what it reports, a player's view, a queue's tracks)
  are the server's Rust types. `cargo test` writes them out as TypeScript into `web/src/lib/generated/` (ts-rs), and
  the app's contract test reads the same files, so a field changed in Rust shows up as a difference everywhere.
- **The rules**: `shared/rules.json` holds the players' rules in one table: Previous's restart, the wind's limits, the
  tape's ranges, the brake and its worked cases. The server's tests, the app's and the page's
  (`web/tests/rules-table.test.ts`) each check their own code against it.
- **The rules crate** (`server/rules/`): what plays and in what order, the brake, how a crate is worked out, what makes
  music alike and Radio's picker, one implementation for the server and for players that play on their own. It reads no
  file, clock or network (`tests/pure.rs`), and `tests/vectors/` holds its cases with their answers, for every build to
  check: crates and Radio are asked of one library there. It also builds to WebAssembly (`web/src/lib/wasm/`), which the
  page can use instead of its own code behind a switch (off by default).

## How the code is shaped

Seven rules, for anything new. They come from ports and adapters, functional core with an imperative shell, and
domain-driven design, taken as far as they pay here and no further.

1. **Decisions live in pure code.** What decides, which copy plays, what a crate holds, how two rows merge, goes in the
   rules crate or a `*Rule.ts` file, with no network or disk, and the time passed in as a value. The code around it
   gathers, calls and writes.
2. **One door per purpose, not per service.** The doors are below, in one word each. A new one needs two real sides, and a test's stand-in
   counts as one. With one side, a plain function will do.
3. **One writer per fact.** `copy_for` alone says which copy plays, and one way changes files (decision 0092).
4. **Identity is an id, never a place.** A path or a place in a list can change. What other things point at needs an id
   for life: a track, a record, an artist, a folder.
5. **One meaning inside each area.** Library, listening, playback, intake and settings each keep their words consistent.
   Where one word crosses areas, say how the two relate.
6. **A rule is guarded in one place.** "Copies hold the same tracks" lives in one function, and other code asks it.
7. **Areas are folders, not services.** One binary. `api.rs` splits along these lines when a change touches it.

Left out on purpose: event sourcing, CQRS across the system, a repository for every table, layers of copied objects,
interfaces with one implementation. The device's index reads a copy and writes through an outbox, which is as far as
that goes.

The platform is three things, and each door belongs to one of them.

**The shared core** is the rules crate: decisions, the same on the server, in the page and in the app.

**The server** owns the truth about the library.

| Door | Purpose | Where |
|---|---|---|
| Sources | What music is there? | `source::Source`: Cratarr, Jellyfin, Subsonic |
| Fetching | Get me this record | `ports::Fetcher`, `ports::Acquisition`: Lidarr, Soulseek |
| References | What is this? | `Asked` for MusicBrainz so far: Discogs, Bandcamp and AcoustID are called one by one |
| Files | Change what is on disk | `ports::Files`, `playlists::Store` |
| Players | Play it over there | Play On, Kodi, Snapcast's `Groups` |
| Scrobbling | Tell others what played | ListenBrainz, Last.fm |

**The clients**, the page and the app, own the experience on a device.

| Door | Purpose | In a browser | In the app |
|---|---|---|---|
| Reads | Server or device? | One decision, and no page asks | The same |
| Writes | Kept here first, sent when Cratarr answers | The outbox, `lib/later` | The same |
| Sound | Make audio | The audio element | ExoPlayer, the tape engine |
| Storage | What is kept here | IndexedDB, caches | Files, `Prefs` |
| Device | Notifications, lock screen, battery, the background | Very little | Android |

In the app the page is the interface and the app gives it what only a device can do, through the bridge: from the page, the
bridge is the Device door. Kotlin holds no decisions. Where the app must decide alone, as Autoplay does with the page asleep,
it asks the rules crate through UniFFI, so no second copy of the logic grows there.

With Cratarr away a device answers for itself (`rules/src/index/device.rs`): a crate, a Sounds like and what comes next, from what it
keeps, by the rules the server runs, asked in the same JSON by the page (`lib/deviceAnswers`, through the Reads door) and the app
(`DeviceAnswers.kt`). What only the server can say stands as it last said it, in an account's own part (`index::own`). In the app the
one copy is the app's, and the page asks it.

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
  - `runtime` (the server's own settings: trusted proxies, home networks, the guest page);
  - `device_index` and `device_covers` (the library on a device, decision 0101: `/api/index` whole and then what moved, the
    sounds, an account's fits and counts, small covers).
- `src/serve/`: Cratarr as an OpenSubsonic server, and sign-in for everything: sessions, accounts, API keys, LDAP;
  `guest.rs` (the guest page, at home only); `access.rs` and `proxy.rs` (who a request is, and from where).
- `src/catalogue/`: every audio file and its tags, read once and kept up to date. `index_log.rs` is what a device's index
  has to hear of: triggers on the tables it reads, and a print of every row for what moves with no write.
- `src/source/`: where music comes from (Cratarr's own folders; Jellyfin optionally), behind one trait.
- `src/crates.rs`: one's crates as they are kept. The rules that make playlists by themselves are the rules crate's
  (`rules/src/crates.rs`), named here; filters use the same engine.
- `src/similarity.rs`: what makes music alike, for radio, Autoplay and "more like this": the rules crate's, named here.
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
- Search (0.12): `src/lib/matcher.ts` and `matcherLoad.ts` run the rules crate's matcher as its own WebAssembly file
  (`src/lib/wasm/matcher.wasm`, built with `scripts/rules-wasm.sh`; the brake stays in `rules.wasm`, inlined): every
  text is folded first (`normalize('NFKD')`, marks dropped, lower case) as the server folds it, and the file is fetched
  at the page's first idle moment and precached for offline. `src/lib/search.ts` keeps its API over it; the server
  runs the same crate, so both find alike (the shared cases in `server/rules/tests/vectors/matcher.json`). With reads
  from the device, the global search is the index's (`index::find`, in `index.wasm`): the library's tier only, the
  others left out with no notice (decision 0101). It reads inside the module, so a library's tracks never come over to
  the page, and folds there too, as their names lie in its bytes: that file alone carries the NFKD tables. Each row is
  one line of folded text and every word of the query must meet it, as the server's `typos` has it, in the server's
  order (`matcher::rank`, which the server's search takes from the crate). What only trigrams find stays the server's.
- Offline writes (0.12, decision 0087): `src/lib/later.svelte.ts` is the outbox: a write is kept on the device first
  (`keptStore.ts`: IndexedDB, else localStorage), then sent in order with its `Idempotency-Key` and the change's time
  (`X-Cratarr-At`); `changes.svelte.ts` reads the server's change log (`GET /api/changes?since=`) and asks again what
  changed; `playlistOps.ts` turns playlist edits into operations (`POST /api/playlists/{id}/ops`).
- Reads, server or device: `read(fromServer, fromDevice)` in `src/lib/api.ts` is the one door, and no page asks whether it is
  offline. `src/lib/deviceIndex.svelte.ts` says where reads come from and why (`readsFrom`, in `deviceIndexRule.ts`, pure): Cratarr
  out of reach, looked for twice, or no network. The index itself is the rules crate's, as `src/lib/wasm/index.wasm`
  (`indexEngine.ts`), its bytes in `indexStore.ts`, each account's apart; `deviceReads.ts` gives its rows the shapes Cratarr gives.
  `deviceSync.svelte.ts` keeps it up to date in a browser. In the app `DeviceIndex.kt` does, with no page open, and the page reads
  its bytes through `/__device/index` by the same module. Both go by one rule, `index::sync`: what to do, and what each answer and
  each set of bytes came to, so neither decides anything of its own. A device holds three revs on one log, the index's, the
  sounds' and the small covers', and gives no feed another's. The first two feeds are each a whole build on the server, so they are
  asked when `/api/changes` says something moved. The covers' builds nothing, and is asked at every sync, which sets their maker
  going. Covers are plain frames, read as they come (`coverFrames.ts`) and kept one by one: only their head goes to the rule.
  What shows as not here goes by that door too (`lib/away`'s `albumAway`, `rowsAway`), so a blip dims nothing, though what plays goes
  by the first failed request, as a hold does. What is said to be here is held to what is: a browser can drop what it keeps, so
  `offline.svelte.ts` holds its list to Cache Storage, and the app calls a download done only with its bytes in the store. A record's
  small cover answers under its full cover's address (`public/sw.js`), so a page cannot tell: from the index in a browser, and from the
  app at `/__device/index/cover/{record}`, which holds them itself.
- `src/lib/shelf.ts` and `shelfStore.svelte.ts` (0.12, decision 0085 step 2): Downloads' details and small covers in
  the page's IndexedDB, filled from the album views while Cratarr answers; it moves into the Rust core's store with
  the whole library's index (0.13).
- `src/lib/snippet.ts` and `snippetPlayer.svelte.ts` (0.12): a track's snippet, picked from its stored waveform and
  beats, played in its own audio element with this device's music ducked; in the Android app handed to the app
  (`{type:'snippet'}`), which plays it beside its own player.
- `src/lib/columnHead.ts`: what a tap on a column's name orders, for every table.
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
