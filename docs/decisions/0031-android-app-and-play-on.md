# 0031: The Android app and Play On

- Status: accepted
- Date: 2026-10-15

## Context

Home Assistant runs the house's music: the salon tablet's playlists, the rotary knob, announcements, the amp, voice.
Today it does that through Jellyfin's Play On: Jellyfin can command any client session, and Finamp on the tablet and
the phone is that client. With Cratarr serving music itself (0030), Jellyfin and Finamp would go for music, and with
them Play On. The Subsonic API has nothing like it: it runs from client to server only.

So Cratarr must be the player as well, and one HA can command. A page cannot be that: measured on the tablet
(`tools/playon-probe`), a browser page refuses to play before a tap, and with the screen locked it freezes about 75 s
after a pause, after which commands reach nobody. Jellyfin solves this on Android with a thin app: its web pages in a
WebView, and a foreground service that keeps them alive and shows the lock-screen controls. Its Android app is open
source (GPL-2.0).

## Decision

**The app** is Jellyfin's Android app made into Cratarr's, in `android/` (licence: 0029, amended).

- Thin, as Jellyfin's: it shows the Cratarr server's pages in a WebView; the web app is not built into it. The first
  start asks for the server's address, once.
- Kept from Jellyfin: the WebView host, the foreground media service (`RemotePlayerService`: media session,
  lock-screen and notification controls, headset buttons), the bridge the page reports its playback through.
- Removed: Jellyfin's server connect and SDK, its native video player, downloads, Android Auto, Chromecast (and with
  it the libre/proprietary flavours), the database, the Jellyfin branding.
- Done better than Jellyfin, so the lock screen cannot stop the music:
  - a wake lock and a Wi-Fi lock while the page plays (Jellyfin takes one only from the notification's Play button);
  - on a device set to **Stay ready** (the always-charging salon tablet) the service, and both locks, stay while
    idle too, so a command after a long pause still arrives; on the phone, HA wakes the app first (through the
    companion app);
  - the foreground notification starts before the cover is fetched, and is not hidden between tracks (Android 12
    refuses to start it again from a locked screen);
  - audio focus is asked for, and unplugged headphones pause;
  - the page may play without a tap (`mediaPlaybackRequiresUserGesture` off);
  - a crashed page reloads instead of closing the app; the app asks to be left out of battery optimisation.

**The page and the app** talk through a small interface, and the page works the same in a browser:

- page to app: what plays (title, artist, album, cover, position, length, paused), and that nothing plays;
- app to page: commands (`play`, `pause`, `next`, `previous`, `seek`, `stop`, volume), as `window.cratarr.command`.

**Play On** is Cratarr's own:

- Each app registers as a **player** under a stable name set in the app ("Salon tablet", "Phone"), over Cratarr's
  live channel, and reports its state on every change, plus a heartbeat. Only apps are players: a page in a browser
  cannot be relied on to answer.
- Commands go to the server, which passes them to the player by name: play tracks, a playlist or an album, from a
  track and position, in order or shuffled; pause, resume, stop, next, previous, seek; volume, duck and unduck; hand
  over (the queue and position move to another player).
- For Home Assistant: the players with their state, pushed on every change (not polled); the commands above; the
  library calls it needs that Cratarr does not have yet (album tags); one health endpoint. HA's side is planned in
  ha-infra, `docs/plans/music-on-cratarr.md`; every command and reading row there maps to one call here.

## Steps

1. The app stripped to the shell; on the tablet, `tools/playon-probe` through it, in the same five cases.
2. Cratarr's page in the app: the media session from the local player, commands into it.
3. Play On: players, commands, the push and health for HA.
4. HA moves its music automations over (its plan, step 2 onwards).
5. Finamp, the Jellyfin webhook plugin and JellyHA leave the music path; Jellyfin keeps video.

## Consequences

- The tablet and the phone run Cratarr's app instead of Finamp. Until step 4, Finamp stays for HA.
- The app is a second build (Gradle) beside the server's (cargo) and the web app's (npm), and is installed by hand
  (or from a store later).
- Jellyfin's fixes to its app can be taken over (`git subtree pull --prefix=android --squash`), but the app soon
  diverges: most of it is removed.
- The page's playback is Chromium's, in the WebView: the app can keep it alive, not feed it. A stall at a track
  change with the screen locked would be the page's to fix (the next track is buffered ahead).

## Amendment (2026-10-15): the app plays natively

Measured on the tablet through the app (`tools/playon-probe`): the app keeps the device awake, but Chromium freezes a
hidden page 70 to 90 s after its music stops, and only showing the page again thaws it; behind the lock screen even a
woken screen does not. A page can therefore not be the player that Play On reaches after a pause. Finamp, which Home
Assistant drives today, has the same gap for another reason (it leaves the foreground on pause), and HA works around it
by launching Finamp and waking the screen before every command.

So inside the app the music plays **natively**, as Finamp's does, and the page is its face:

- The app's player is Media3's ExoPlayer in a media session service: the lock screen, the notification, headphone
  buttons, audio focus, pausing when the audio would become noisy, and the wake and Wi-Fi locks come with it. It plays
  the original files (`/api/file/{source}/{id}`, with byte ranges), gapless from their own headers (LAME's for MP3).
- The page's local player hands over to it when the page runs in the app: the same calls (play a queue, add, move,
  remove, jump, next, previous, seek, play/pause, stop) go over the bridge, and the app reports back what plays. In a
  browser the page plays itself, as before.
- Play On reaches the app's player, not the page: the app holds the connection to Cratarr (step 3), so a frozen page
  does not matter; the page catches up with what the player did when it is shown.
