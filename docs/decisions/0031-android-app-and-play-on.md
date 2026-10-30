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

## Amendment (2026-10-16): the sleep timer

The player keeps the sleep timer, as it keeps repeat: only the device that plays is sure to be awake when it fires
(behind a locked screen, offline). Command `{"command": "sleep", "at_ms": <epoch ms> | null, "end_of_track"?: true}`
(both empty clears it); the page's bridge op is `{"op": "sleep", "atMs", "endOfTrack"}`. The player's reports and
view carry `sleep: {at_ms, end_of_track} | null`. When it fires, the player pauses, as the pause button does, and the
timer clears; one that runs out while paused only clears. A handover takes it along; a stop ends it. A page that plays
in the browser keeps its own.

## Amendment (2026-10-16): two addresses, the Network settings

The owner, from a phone on a VPN to the home router that carries only the home network: Cratarr's public address led
past the VPN to the reverse proxy, which serves the home network only, and everything was refused. So the app knows two
addresses, as Finamp's Network settings do (its redesign: "Public Address", "Prefer Local Network", "Local Network
Address", "Active Address", "Test both connections"), in a Network part of the settings of its own (app only):

- **Public address**: the one used from anywhere (the app's only address before).
- **Prefer local network** and **Local network address**: while on, the local network address is used whenever it
  answers (at home, or over such a VPN), else the public one; when neither answers, the one in use stays.
- **Active address** shows which, and **Test both connections** tries both.

Answering is tried (Cratarr's health check), as Finamp does, rather than the home network told from the Wi-Fi's name
as Home Assistant's app does: that needs location access, and misses a VPN. The app looks again when it starts, on
every change of network, when it comes back to the front, and when its connection to Cratarr fails; a page that cannot
load tries the other address before the "Cannot reach" dialog. Everything follows the address in use: the page, Play
On's connection, radio, levelling, plays told, and what was set up on the other address (queued tracks, downloads),
read from this one. The page's own settings (`cratarr-prefs`) live in the browser storage of its address, so the app
keeps a copy (sent on each change) and puts it into the page's storage before the page starts, on either address.

**Signed in away from home (2026-10-16, the owner):** the reverse proxy now asks for a Cratarr account (HTTP Basic, the
Settings → Authentication accounts, through `/api/auth/check`) off the home network. Settings → Network gains User
name and Password (the password never shown; a typed one replaces it). The app sends them with each of its own
requests to the public address (`Net.kt`: the player's and the downloads' too; the local network address needs none),
and gives them to the page's WebView when the proxy asks; with none kept, or a refused one, it asks in a native dialog
(the page cannot load to show its settings), and keeps what is typed. A refusal (401, or a 403) holds the app's
requests to the public address back for ten minutes or until the sign-in changes: polling on regardless got the
owner's phone banned as a prober by the proxy's CrowdSec the first evening.

**Replaced (2026-10-17) by decision 0039:** Cratarr's own sign-in page and sessions. The app keeps no password any
more (Settings → Network's User name and Password, and the native dialog, are gone); its own requests carry the
WebView's session cookie, and the refusal hold now lasts until the page signs in again.

## Amendment (2026-10-16): a page in a browser is a player while it is open

The owner, with the tablet away from home playing in its browser: "It should" be a Play On target. Decision 0033 had
it ("a Play On target only while it is open"); the line above ("Only apps are players") gives way to it. A page that
plays here (its output is this browser) joins Play On as a player of kind `browser` (`/api/players/{name}/connect?kind=browser`,
`lib/playOnTarget`), under this browser's name (Settings → This device, which a browser shows too: "Chrome on Android"
until named; a second one of the same takes "… 2"). Cratarr's commands play on the page's own player, as the app's;
it reports what plays on every change and every 5 s, and gets its queue back after a reload, as the app does. What a
browser allows, kept to:

- Sound starts only once the page has been touched (sticky per page, until it reloads): a play before that shows a
  "Tap to play" bar.
- A page paused behind a locked screen is frozen after a minute or so (measured on the tablet, the reason the app plays
  natively): it shows offline then, and is a player again when looked at. Playing, it stays.
- No sync play: a browser joins no group, and none joins it.
- A connection refused is tried again after 5 s, doubling to a minute: never in a rush, as a proxy bans what keeps
  knocking.

## Amendment (2026-10-17): the players as a stream

`GET /api/events/players` sends `players` (server-sent events), the same JSON as `GET /api/players`, at once and on
every change of a player; each carries `position_ms` with `position_at` (epoch ms), so a client counts on from there
while `playing`. It is for clients that stay connected for good, Home Assistant's integration first: unlike the pages'
`/api/events` it does not keep the servers' players watched. Signed in as any other API call (decision 0043: an API
key). Not under `/api/players/`, where a player could be called `events`.

## Amendment (2026-10-19): ready in a service of its own, and a start the device refuses

The salon tablet, set to Stay ready and idle, took a handover from the phone at 12:17 and paused 0.4 s later:
Android (16; from 15 on) lets an app start sound only while it is on the screen or holds a foreground service, and
ignores its request otherwise (`dumpsys audio`: "AudioHardening focus request … ignored … level: partial", that is,
no foreground service at all). The player's own service held Stay ready's foreground, and Media3 takes that service in
and out of the foreground as the music plays and stops, at times on an update of its own the app does not see; an
update of the app left it out too, until someone opened it.

- **Ready is a service of its own** (`ReadyService`, the "Ready" notification), apart from the player's, which Media3
  keeps as it does. It is asked for whenever the app starts, comes to the front, connects, or finds it gone (every
  30 s), and by itself after an update of the app or a restart of the device. Its failures go to the link log.
- **A start the device refuses**, the player pausing for want of the sound within 3 s of Cratarr asking it to play, is
  asked again once the app is in the foreground (the Ready service; for a minute only without Stay ready). Refused a
  second time, it is **held back**: the player reports `held` (why, as text; null otherwise), shown in its view
  (`GET /api/players`), a notification asks to tap to play, and the music starts as the app comes to the front, for
  10 minutes. A pause, a stop or another play lets it go.
- Covers (the notification's, the lock screen's, a car's) are read through the app's own requests, with its session:
  away from home the proxy refused Media3's own.

## Amended (2026-10-23): Android's media controls follow the screen

The phone's page drove the Salon tablet while Android's own media controls (the notification, the lock screen, the
output chip) said "This phone" with a paused track of its own (the owner: "It would be awesome if we could see here …
that something is being played on the tablet instead"). As decision 0033's "one screen, one place": while the page
plays on another of Cratarr's players, the app's media session shows that one.

- **The page says which** (`{"type": "drive", "player": name | null}`, `lib/playOn`), as it opens and as its output
  changes, before it asks the app's player for its state again. The app keeps it (`Prefs.driven`) and starts its
  service for it.
- **The session's player is swapped** (`MediaSession.setPlayer`): while driving, a `RemotePlayer` (Media3's
  `SimpleBasePlayer`) mirrors that player from Cratarr's live channel of its players (`/api/events/players`): the
  track, its cover, playing or paused, the position counted on from the report on Cratarr's clock. Play, pause, next,
  previous, seek and the volume keys become Play On commands. Its playback is remote (`PLAYBACK_TYPE_REMOTE`, volume 0
  to 100). Driving none again, the app's own player is the session's again.
- **Step 2, not yet:** the chip names the device only through a routing session (SystemUI's `MediaDeviceManager`:
  without one it shows "Other device"), which needs the app to offer Cratarr's players as routes
  (`MediaRoute2ProviderService`), as Cast does.

## Amended (2026-10-23): one volume, Android's

An app player had a volume of its own: Play On's `volume` set a factor on its player, kept in the app's memory, shown
by nothing on the device; Android's slider and keys could not raise it. A test left the phone at 0 while everything
said it played at full volume (the owner: "I'm afraid that we have a hidden volume somewhere"; the tablet played at 58
of its own under Android's 15 of 15). Now Play On's `volume` sets Android's media volume (its own steps, the nearest),
and the reports say Android's media volume, watched as it changes (the keys, the slider): the system slider, the keys,
Home Assistant, a group's sliders and the media controls of a device that drives it move one and the same. The player
plays at the duck times the track's levelling only (decision 0035). In a browser, which has no slider of its own for a
page, the Play On sheet shows the page's volume under This device.
