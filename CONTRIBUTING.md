# Contributing

Cratarr is a Rust server, a Svelte web app built into it, an Android app that plays natively around that page, and a
desktop app (Electron) around the same page. This is how to build it, check a change, and propose one.

## Building and running

- **Web app** (`web/`): Node 22 and pnpm. `pnpm install`, then `pnpm build` (the server and the Android app serve
  `web/dist`, so build it before them), or `pnpm dev` for Vite's dev server on port 5173, which sends `/api` to a running server
  (`CRATARR_URL`, by default `http://127.0.0.1:8080`).
- **Server** (`server/`): stable Rust. `cargo run` serves on port 8080. Set `CRATARR_DATA_DIR` to keep settings and
  databases between starts; the other variables are in the [README](README.md#configuration). It reads the page from
  `web/dist` (`CRATARR_WEB_DIR`) on every request, so a new `pnpm build` shows without rebuilding the server.
- **The image**: `docker build -t cratarr:local .` at the top of the repository builds both: the static binary and the
  page beside it in an otherwise empty image.
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
- Send a pull request against `main`, one task each, with the checks above run. `main` is the last release, so a
  change made on it fits the release it names.
- **Commits**: one subject line that says what changed for the person using Cratarr, then why, and anything a reader
  of the code later needs: what was measured, what was tried and dropped, which decision it follows. Tests that pin
  the behaviour come in the same commit as the change.
- **Licences**: what you contribute is under the AGPL-3.0-or-later ([LICENSE](LICENSE)), and under the GPL-2.0 in
  `android/` ([android/LICENSE.md](android/LICENSE.md)). Code copied or translated from another project keeps a notice
  naming its origin, copyright holder and licence, and says what was changed
  ([decision 0030](docs/decisions/0030-cratarr-serves-opensubsonic.md)).

## How the repository is kept

- `main` is what was released, and moves only at a release. GitHub's `main` gets one commit per release, its tree the
  release's, made with `git commit-tree`; no development branch is published.
- `trunk` is the development branch: the next feature release (0.11 while 0.10 is out). It holds all of Cratarr: the
  server, the web app, the Android and desktop apps and the docs.
- `release/X.Y` holds a released version and its fixes only (`release/0.10`: 0.10.0, 0.10.1, …). It is cut from `trunk`
  at the very commit released as X.Y.0, and nothing of `trunk` ever comes into it.
- Work happens on branches named for it:
  - a feature from `trunk`, and back into `trunk`;
  - a fix from the oldest `release/X.Y` it belongs in, back into that, released from there as X.Y.Z+1; then that
    release branch is merged forward into `trunk`, so the next version has every fix.

  A branch comes back with a merge, never as a copy of its files (no cherry-picks either), so the history tells what is
  where. There the pieces are checked together and run on a real install before they are released. A working branch
  takes its own base (`trunk` or its release branch) back in whenever it needs what landed there.
- A pull request: GitHub's `main` shares no history with `trunk` (it is one commit per release), so its change is taken
  onto `trunk` (a feature) or the release branch (a fix) as a commit of its own, its author kept, and comes back to
  `main` with the next release.
- What makes a fix and what a new version is not which part it touches, but whether stepping back undoes it:
  - a fix (X.Y.Z+1), in any part (the server, the web app, the apps), mends what is there: no change to the data
    kept on disk (no migration), nothing an app or page of the same X.Y would misread in what the API sends or expects,
    no new feature. The release before it can always be gone back to without losing anything;
  - a new version (X.Y+1.0) is everything else: new features, changed behaviour, migrations, changes between the
    server and its apps. A fix that would need a migration goes there, or is made without one.
- A release is its branch checked as a whole, with the version raised in one commit (`scripts/version.sh`: plain
  `X.Y.Z`, the server, the web app, the Android and desktop apps), tagged `vX.Y.Z` and put on `main` as one commit.
  - A new version (X.Y.0) is made on `trunk`, and `release/X.Y` is cut there.
  - A fix (X.Y.Z) is made on `release/X.Y`.
  - The tag builds the image for amd64 and arm64 and makes the GitHub release
    ([.github/workflows/release.yml](.github/workflows/release.yml)), so what runs can always be found again.
  - Once X.Y+1.0 is out, `release/X.Y` gets no more fixes.
  - Builds before 0.8 were tagged `preview-N`.
