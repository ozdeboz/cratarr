# Cratarr for the desktop

A thin app around Cratarr's pages, so a computer can be a Cratarr player that starts at login and keeps playing with
its window closed (decision [0078](../docs/decisions/0078-a-desktop-app.md)). On Linux it is a `.deb` for Ubuntu,
Linux Mint and Debian, built where it is used: it is not published (the owner, 2026-11-04: local only). A Windows
installer can be built the same way (`npm run dist:win`) but has not been built or tried yet.

## What it does

- Shows Cratarr's page on the server's address; the page plays, as in a browser. The server's address is asked once;
  **Change server…** (in the window's menu, shown with Alt, and in the tray) asks again.
- Carries its own copy of the page (decision 0085, step 1, as the Android app does): it answers on the server's address
  in place of the server's page files, so the window opens at once, and with the server out of reach too, never
  blank. Which copy, by the two builds (`build.json`; the server's from its health check, every ten minutes and on
  waking): the server's release newer, or the same release built again, the server's; otherwise the app's. The API,
  the streams and the sign-in always go to the server, and so do the backdrops' and skies' pictures (kept by the
  page's service worker once seen).
- Keeps what the page stores on the computer (its last copies, its downloads, the library's index) for good: a browser
  lets go of a site's storage when the disk runs low, the app asks for it to be kept.
- Plays without a first click, and keeps its timers with its window hidden, so music sent from another player sounds
  at once.
- Closing the window leaves it in the tray, still playing and still a player; **Quit** ends it. **Start at login**
  (tray and menu) starts it with its window hidden.
- Works over https and over plain http (a server's home-network address): there that one address is treated as
  secure, so Downloads work as on https.
- The player's first name is the computer's name (Settings → This browser renames it).
- Another site's links open in the computer's browser.

## Installing

Build the `.deb` (below), then install it:

```sh
sudo apt install ./desktop/dist/cratarr-desktop_<version>_amd64.deb
```

A copy already running is told to quit and starts again from the new files, as hidden as it was. Each Cratarr
release, the app is built again at its version and installed the same way: there is no APT repository and no
updater.

## Building

Node 24 and npm (not pnpm: pnpm 10 skips Electron's download; if npm skips it too, `node node_modules/electron/install.js`
fetches it). The page the app carries is the web app's build of the same release: build it first.

```sh
(cd web && corepack pnpm install --frozen-lockfile && corepack pnpm build)   # web/dist, the page the app carries
cd desktop
npm ci
npm start           # runs it; the web app's icons and page are copied in first
npm test            # which page answers, and how (page.js)
npm run dist        # on Linux: dist/cratarr-desktop_<version>_amd64.deb
npm run dist:win    # on Windows: dist/Cratarr-Setup-<version>.exe
npm run check       # the check each build runs first: no private IP address or home-network host name in the app
```

Without a page of the same release in `web/dist`, the app is built without one and says so (it then shows the
server's page, as before); `CRATARR_PAGE=required npm run dist` makes that a failure.

The `.deb` depends on electron-builder's list plus what Electron needs that it leaves out: the sound library
(`libasound2t64` from Ubuntu 24.04, `libasound2` before) and `libgbm1`.

## Files

- `main.js`: the window, the tray and the menus, Start at login, the server's address; the page's requests, answered by
  the app's own page or sent on to the server, and the server's page as its health check tells it.
- `page.js`: the app's own copy of the page, which copy answers and how (`page.test.js`: `npm test`).
- `copy-page.js`: the web app's build (`web/dist`, without the backdrops and skies) copied into `page/`, which the
  package carries as `resources/page`; only a build of the app's own release.
- `preload.js`: `window.CratarrDesktop` for the page (the app's version, the computer's name); the address page's two
  questions.
- `setup.html`: the address page, the app's one page of its own.
- `copy-icons.js`: the web app's icons, copied in at 0644 (a copy keeps its checkout's mode).
- `after-pack.js`: before packing, the app's files writable by their owner alone. Electron's own come out of its archive
  writable by anyone, and a package installs them as they are.
- `after-install.tpl`: the `.deb`'s after-install, electron-builder's own with one step more: a copy still running
  from the replaced files is told to quit (killed if it does not) and started again from the new ones, in its user's
  session and as hidden as it was.
- `check-deb.js`: after the packages are made, each `.deb`'s list read; any entry writable by group or others stops
  the build. `npm run dist` builds under `umask 022`.
- `check-no-house.js`: the check before each build; a house's own names come from `CRATARR_HOUSE_PATTERNS` (an
  environment variable where the build runs), so they never sit in this repository.
