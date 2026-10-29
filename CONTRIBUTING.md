# Contributing

Cratarr is a Rust server, a Svelte web app built into it, an Android app that plays natively around that page, and a
desktop app (Electron) around the same page. This is how to build it, check a change, and propose one.

## Building and running

- **Web app** (`web/`): Node 22 and pnpm. `pnpm install`, then `pnpm build` (the server embeds `web/dist`, so build it
  before the server), or `pnpm dev` for Vite's dev server on port 5173, which sends `/api` to a running server
  (`CRATARR_URL`, by default `http://127.0.0.1:8080`).
- **Server** (`server/`): stable Rust. `cargo run` serves on port 8080. Set `CRATARR_DATA_DIR` to keep settings and
  databases between starts; the other variables are in the [README](README.md#configuration). A debug build reads
  `web/dist` from disk on every request, so a new `pnpm build` shows without rebuilding the server.
- **The image**: `docker build -t cratarr:local .` at the top of the repository builds both into one static binary
  in an otherwise empty image.
- **Android app** (`android/`): see [android/README.md](android/README.md#building) (Android Studio's JDK and the
  Android SDK, then `./gradlew assembleDebug`).
- **Desktop app** (`desktop/`): see [desktop/README.md](desktop/README.md) (Node 22, `npm ci`, then `npm run dist`).

## Before a change goes in

- **Server**: `cargo check` and `cargo test` in `server/`: the whole suite, not only the part changed, and no new
  warnings. Both, because a test build can compile what a real build cannot (a method kept for tests only, used in
  the server).
- **Web**: `pnpm check` in `web/` (svelte-check, no errors or warnings) and `pnpm test`, and the change looked at in a
  browser, at a phone's width too where it shows there (`pnpm check:phone` checks that no page scrolls sideways at
  390 px).
- **Android**: `./gradlew testDebugUnitTest assembleDebug` passes and builds.
- **Desktop**: `npm run check` in `desktop/` (no address of one house in the build).
- **Decisions**: choices that shape the product or its data are written down first in
  [docs/decisions](docs/decisions) (`NNNN-slug.md`: status, date, context, decision, consequences), and a later change
  to one is a dated amendment to it. Bigger ideas that are not decided yet go in [docs/plans.md](docs/plans.md).
- **Pages** follow [docs/ui-conventions.md](docs/ui-conventions.md).

## Proposing a change

- For anything beyond a fix, open an issue first, so the decision behind it can be talked through before the work.
- Send a pull request against `main`, one task each, with `main` brought in and the checks above run.
- **Commits**: one subject line that says what changed for the person using Cratarr, then why, and anything a reader
  of the code later needs: what was measured, what was tried and dropped, which decision it follows. Tests that pin
  the behaviour come in the same commit as the change.
- **Licences**: what you contribute is under the AGPL-3.0-or-later ([LICENSE](LICENSE)), and under the GPL-2.0 in
  `android/` ([android/LICENSE.md](android/LICENSE.md)). Code copied or translated from another project keeps a notice
  naming its origin, copyright holder and licence, and says what was changed
  ([decision 0030](docs/decisions/0030-cratarr-serves-opensubsonic.md)).

## How the repository is kept

- `main` is what was released, and moves only at a release.
- Work happens on branches named for it, each from `trunk`, the development branch, which holds all of Cratarr: the
  server, the web app, the Android and desktop apps and the docs. A branch comes back with a merge, never as a copy of
  its files, so the history tells what is where; there the pieces are checked together and run on a real install
  before they are released, and a working branch takes `trunk` back in whenever it needs what landed there. A pull
  request against `main` is brought onto `trunk` the same way.
- A release is `trunk`, checked as a whole, with the version raised in one commit (`server/Cargo.toml` and its lock,
  `web/package.json`, the Android and desktop apps), tagged `vX.Y.Z` and brought onto `main` (a fast-forward). The tag builds the image for amd64 and arm64
  and makes the GitHub release ([.github/workflows/release.yml](.github/workflows/release.yml)), so what runs can always
  be found again. Builds before 0.8 were tagged `preview-N`.
