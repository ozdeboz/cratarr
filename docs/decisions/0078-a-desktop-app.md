# 0078: A desktop app: Cratarr's pages in a window of their own, Linux first

- Status: accepted (the owner, 2026-10-29: "Could we easily create a Linux app first for Linux Mint / Ubuntu?"; of
  the installed page, a thin shell and a full app, the thin shell)
- Date: 2026-10-29
- Builds on: 0031 (the Android app, thin at first), 0033 (a page is a player while it is open), 0.9.8 (Downloads in
  the installed page)

## Context

On a Linux computer the web app already installs as an app from Chrome or Chromium: a window and a launcher of its
own, Downloads, the desktop's media controls (the page's media session), and Play On while it is open. A browser
holds it back where a computer is meant to be one of Cratarr's players:

- It installs only over https: a server reached at a home-network address over plain http gets no service worker, so
  no install and no Downloads.
- It plays only after the page was clicked once (the browser's autoplay rule): a play sent from another player shows
  "Tap to play".
- Closed, it is no player; it does not start with the computer; a covered window's timers are slowed.
- One install holds one server.

A full app, playing natively as the Android app does since 0031 (its own engine, the tape, the local library), means
porting the Android app's player: a big job.

## Decision

**A thin desktop app (Electron), as the Android app began: it shows the server's own pages in a window of its own,
and the page plays as in a browser. Linux first, as an AppImage and a .deb.**

- Nothing of the web app is built in. The pages come from the server, so the app follows each server's version.
- The server's address is asked once, on the app's one page of its own; the health check (`/api/health`, open to
  all) tells a Cratarr server from anything else. **Change server…** (the window's menu, shown with Alt, and the
  tray's) asks again; the app then starts afresh with it.
- What the app adds to the installed page:
  - The autoplay rule is off: a play sent from another player sounds at once.
  - A hidden or covered window keeps its timers (its reports to Cratarr, the next track).
  - Closing the window leaves it running in the tray, playing and a player; **Quit** ends it. **Start at login**
    (tray and menu) starts it with its window hidden: on Linux an XDG autostart entry, elsewhere the system's login
    item.
  - A plain-http server is treated as a secure origin (Chromium's `unsafely-treat-insecure-origin-as-secure`, for that
    one origin), so the service worker and Downloads work there as on https.
- The page knows the app by `window.CratarrDesktop` (its version, the computer's name): there it counts as installed
  (Downloads), and the computer's name is the player's first name. Otherwise it is the browser's page: Settings →
  This browser, no `CratarrApp` bridge, the page's own player.
- The window is the server's alone: another site's link opens in the computer's browser. The page may copy, go
  fullscreen, keep the screen on and choose a speaker; nothing else (no camera, microphone or location).
- The release workflow builds both packages at the release's version and attaches them to the GitHub release.

## Consequences

- The app's own code is small (`desktop/`: the window, the tray, the menus, the address page); the page stays one.
- The same shell builds for Windows and macOS when wanted (electron-builder), unsigned for now.
- Not in it, as in a browser: leading a sync group (0037), the app's tape engine, the local library (0052). They stay
  the Android app's.
