# Cratarr for the desktop

A thin app around Cratarr's pages, so a computer can be a Cratarr player that starts at login and keeps playing with
its window closed (decision [0078](../docs/decisions/0078-a-desktop-app.md)). Linux first: a `.deb` for Ubuntu, Linux
Mint and Debian, and an AppImage for the rest.

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

- `.deb`: `sudo apt install ./cratarr-desktop_<version>_amd64.deb`, then Cratarr in the menu (or `cratarr`).
- AppImage: `chmod +x Cratarr-<version>.AppImage`, then run it.

Both are in each [release](https://github.com/ozdeboz/cratarr/releases).

## Building

Node 22 and npm (not pnpm: pnpm 10 skips Electron's download).

```sh
cd desktop
npm ci
npm start       # runs it; the web app's icons are copied in first
npm run dist    # on Linux: dist/Cratarr-<version>.AppImage and dist/cratarr-desktop_<version>_amd64.deb
```

The release workflow builds both at the release's version and attaches them to the GitHub release. A hotfix's tag
(`v0.9.15-1`) is built as 0.9.15 with the hotfix's number as the `.deb`'s revision (`0.9.15-1`), so apt sees it above
0.9.15; built as the tag says, the `.deb` would carry `0.9.15~1`, below it.

The `.deb` depends on electron-builder's list plus what Electron needs that it leaves out: the sound library
(`libasound2t64` from Ubuntu 24.04, `libasound2` before) and `libgbm1`.

## Files

- `main.js`: the window, the tray and the menus, Start at login, the server's address.
- `preload.js`: `window.CratarrDesktop` for the page (the app's version, the computer's name); the address page's two
  questions.
- `setup.html`: the address page, the app's one page of its own.
- `copy-icons.js`: the web app's icons, copied in.
