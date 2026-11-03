# 0098: Modes per server — a Jellyfin or a Subsonic server in the app

- Status: accepted (the owner, 2026-11-03, by question set in 632's session); for 0.15.
- Date: 2026-11-03
- Builds on: 0052 (local mode: a mode of its own, never mixed, each with its own queue), 0085 (offline first), 0091
  (scrobbling from the device, and its switch per server), docs/plans.md (External library; modes only, settled
  2026-10-28)
- Leads to: all servers in one library, on the device (docs/plans.md), as a view over the modes' indexes

## Context

The owner, 2026-10-28: "because of our heavy feature set, we will offer extra servers only as modes". Each other
server is a mode of its own beside Cratarr's and Local, never mixed: no album matched across servers, no user data
mixed, no queue holding a track the next player cannot reach. What was still open was how such a mode reaches its
music (through Cratarr's server, or in the app), and what it offers.

## Decision

**In the app, as Local.** The app signs in to a Jellyfin or a Subsonic server itself and streams from it directly. It
works where only the device reaches that server, and with no Cratarr server at all; it is the base the merged library
on the device builds on. The mode plays on this device only: no Play On, groups or Home Assistant. Its sign-ins are kept
on the device (Android's keystore, the desktop app's store), never sent to Cratarr's server. Only in the apps: a browser
page holds one origin's sign-in.

**Added in Settings › This device › Servers.** Cratarr's server first, then each added server: its address, then its
own sign-in, which the user fills in (Jellyfin's Quick Connect where the server offers it). The mode switcher
(lib/ModeSwitcher) shows as soon as there is more than one mode, and lists them all: Cratarr, each server by its name,
Local.

**What a server's mode has.** Its library and search, its playlists, Home, Radio and Autoplay, and Cratarr's players
(the device's output, Start and stop, the tape, levelling); never crates, Discover, Insights, Requests, Health or the
Edit sheet.

- *Playlists:* made, renamed, reordered, added to and taken from on that server, through its own API (Subsonic's
  `createPlaylist`/`updatePlaylist`, Jellyfin's playlists), so they stay the same in its other apps.
- *Home:* that server's lists, on Cratarr's Home row cards: Recently added, Recently played, Most played, Favourites,
  Random (Jellyfin's, Subsonic's `getAlbumList2`). No crate cards.
- *Radio and Autoplay:* that server's own mix (Jellyfin's Instant Mix, Subsonic's `getSimilarSongs2` where the server
  has it); where it has none, no Radio in that mode. Its sheet offers only what that mix can do.
- *Favourites and plays:* that server's (its stars, its play reports), never Cratarr's history or counts. Scrobbling
  goes as 0091 has it: this device's, or the server's own where its "This server scrobbles for me" is on.

**One queue per mode, one mode sounds at a time** (0052): switching pauses what plays and brings the other mode's
queue back, paused. Downloads are kept per server.

**Settings:** the device's own (output, Start and stop, the tape, levelling) and its look (Appearance) shared across
modes; Home per mode: in a server's mode, Settings › Content orders and switches that server's rows (the owner,
2026-11-03: "Shared; only Home per mode").

**In the car:** Android Auto browses the mode the app is in; modes are switched on the device. (The app has no browse
tree yet, only its media session, which shows and steers what plays in any mode: this holds once the car browses.)

**Not in 0.15:** another Cratarr server as a mode (the owner, 2026-11-03).

## Consequences

- The page gains a library view fed by the app's adapter rather than Cratarr's server, as local mode's
  (lib/deviceMusic) is; the menus offer only what that server can do (the owner's rule: nothing offered that cannot be
  upheld).
- Waveforms, tempo, 'Sounds like' and AudioMuse's facts are absent for those tracks; nothing stands in for them.
- The app's player streams other servers' URLs with their own sign-in, and reports plays to them.
