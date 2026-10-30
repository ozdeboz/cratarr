# Cratarr for the desktop

A thin app around Cratarr's pages, so a computer can be a Cratarr player that starts at login and keeps playing with
its window closed (decision [0078](../docs/decisions/0078-a-desktop-app.md)). On Linux it comes as a `.deb` for Ubuntu,
Linux Mint and Debian, from an APT repository so it updates with the system. A Windows installer that updates itself
is prepared (`npm run dist:win`, the release workflow's Windows job) but has not been built or tried yet.

## What it does

- Shows the Cratarr server's pages; the page plays, as in a browser. The server's address is asked once; **Change
  server…** (in the window's menu, shown with Alt, and in the tray) asks again.
- Plays without a first click, and keeps its timers with its window hidden, so music sent from another player sounds
  at once.
- Closing the window leaves it in the tray, still playing and still a player; **Quit** ends it. **Start at login**
  (tray and menu) starts it with its window hidden.
- Works over https and over plain http (a server's home-network address): there that one address is treated as
  secure, so Downloads work as on https.
- The player's first name is the computer's name (Settings → This browser renames it).
- Another site's links open in the computer's browser.

## Installing

Ubuntu, Linux Mint, Debian: from the APT repository, so the Update Manager (or `apt upgrade`) brings each release. The
repository is made by the first published release; until then, build the `.deb` as below and install it with `apt
install ./dist/cratarr-desktop_<version>_amd64.deb`.

```sh
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://ozdeboz.github.io/cratarr/apt/cratarr-archive-keyring.gpg | sudo tee /etc/apt/keyrings/cratarr.gpg > /dev/null
echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/cratarr.gpg] https://ozdeboz.github.io/cratarr/apt stable main" | sudo tee /etc/apt/sources.list.d/cratarr.list
sudo apt update && sudo apt install cratarr-desktop
```

Windows: not yet. The release workflow has a job for `Cratarr-Setup-<version>.exe`, an installer that would update
itself from the releases (unsigned, so Windows would ask once before it runs), but it has not been built or tried.

Each release's `SHA256SUMS` holds its files' checksums.

## Releasing

A version tag builds the app with the rest of the release (`.github/workflows/release.yml`): the `.deb` and the
installer join the GitHub release, the APT repository on GitHub Pages is made anew, and `SHA256SUMS` comes last. Each
is built at the release's version, so apt and the updater see every release as newer. A hotfix's tag (`v0.9.15-1`)
is built as 0.9.15 with the hotfix's number as its build number: the `.deb`'s revision (`0.9.15-1`, which apt sees
above 0.9.15; built as the tag says, it would carry `0.9.15~1`, below it) and Windows' file version.

Once, before the first release:

1. The APT repository's signing key: `desktop/make-apt-key.sh`, run on your own computer with gpg and gh signed in as
   the repository's owner. It sets the secret `APT_SIGNING_KEY`, writes a backup to keep in your password manager, and
   shows the key's fingerprint, which the repository's page shows too.
2. GitHub Pages: Settings → Pages → Source: GitHub Actions; then Settings → Environments → github-pages → let tags
   `v*` deploy.
3. If you like, the secret `CRATARR_HOUSE_PATTERNS`: your own host names and domains, as comma-separated regular
   expressions, which the build's check refuses too.

## Building

Node 24 and npm (not pnpm: pnpm 10 skips Electron's download).

```sh
cd desktop
npm ci
npm start           # runs it; the web app's icons are copied in first
npm run dist        # on Linux: dist/cratarr-desktop_<version>_amd64.deb
npm run dist:win    # on Windows: dist/Cratarr-Setup-<version>.exe, with latest.yml and its blockmap for the updater
npm run check       # the check each build runs first: no private IP address or home-network host name in the app
```

The `.deb` depends on electron-builder's list plus what Electron needs that it leaves out: the sound library
(`libasound2t64` from Ubuntu 24.04, `libasound2` before) and `libgbm1`.

## Files

- `main.js`: the window, the tray and the menus, Start at login, the server's address, Windows' updates.
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
- `check-no-house.js`: the check before each build; a house's own names come from `CRATARR_HOUSE_PATTERNS`, so they
  never sit in this repository.
- `apt-repo.sh`: the APT repository from a `.deb`, signed, as the release workflow deploys it to GitHub Pages.
- `make-apt-key.sh`: the repository's signing key, made once by its owner.
