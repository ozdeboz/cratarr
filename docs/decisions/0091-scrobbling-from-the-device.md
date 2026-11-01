# 0091: Scrobbling from the device

- Status: accepted (the owner, 2026-10-31: "scrobbling should indeed move to the device! This also makes more sense
  privacy wise", and the choices below, by question set; the server's parts reviewed by a5 the same day); for 0.13,
  with modes per server.
- Moved: with modes per server to 0.15 (the owner, 2026-11-01: 0.13 frozen at what preview.3 holds; docs/plans.md).
- Date: 2026-10-31
- Builds on: 0068 (one way in for listening; its scrobbler), 0064 (the listening history), 0085 and 0087 (offline
  first; plays with an id and a time), 0051 (several users), modes per server (docs/plans.md, 0.13)
- Amends: 0068 (who sends to the listening services)

## Context

Cratarr's server scrobbles today: every play it counts, from its own players, Subsonic apps and Kodi, is queued in the
same transaction that logs it and sent to the ListenBrainz and Last.fm connections the account made (0068; the keys in
`scrobble_target.secret`). That has two costs:

- **Privacy.** The server holds every user's service keys and sends everyone's listening out.
- **Other servers' tracks.** With modes per server (0.13), and later all servers in one library on the device
  (docs/plans.md), a device plays tracks from a Jellyfin or a Subsonic server that Cratarr's server never sees. The
  server cannot scrobble what it does not hear about.

The owner wants the device to scrobble, but did not want to lose the convenience of signing in once.

## Decision

**Each user's choice, in Connections: from each device (the start), or through the server.**

- **From each device.** The player that sounds a track tells ListenBrainz and Last.fm itself, for every server's
  tracks. Each device keeps its own sign-in to each service and its own queue of scrobbles while offline (Last.fm takes
  them up to 14 days late; ListenBrainz any time). The server holds no service keys for the account and sends nothing
  out for it.
- **Through the server.** As 0068 has it, signing in once for all devices. Cratarr's own tracks are scrobbled from the
  plays the server already counts (`/api/plays`, Subsonic's `scrobble`, Play On's reports). A device also hands the
  server its plays of other servers' tracks, which the server sends but does not count in its history (they are not
  its tracks).
- A new account starts on "from each device". An account that has a service connected when this arrives stays
  "through the server" until it is changed, so no one's scrobbling stops unannounced (the owner, the same day).

**Who tells a play: the player that sounded it, once.**

- The app's own player and the desktop app's: each is a device. The page's player in a browser is one too.
- A group: its leader, never its followers.
- Play On: the player it plays on, never the device that started it.
- Subsonic apps and Kodi, which play from Cratarr without Cratarr's app, follow the user's choice. Through the
  server, the server scrobbles their plays as now. From each device, the server has no keys, and those apps scrobble
  themselves (Symfonium and Kodi have their own Last.fm options) or not at all.
- Each scrobble carries its play's id (0087), so the same play is never sent twice, whichever way it goes.

**What stays.** Cratarr's own listening history (0064), its counts (0022) and the imports (0067) do not change: a play
of Cratarr's tracks is reported to Cratarr's server as now, whichever way it is scrobbled. Tracking off (0068's
amendment) stops scrobbling on every path. A device goes by the account's `tracking` as it last read it (offline it
cannot ask), and reads it again before it sends its queue; what it queued while tracking was off is dropped, never
sent later.

## Server (for review)

- The account's choice: `settings.scrobble` = `device` | `server`, read with the account's settings.
- `device`: the account's `scrobble_target` rows are removed, and nothing is queued for it in `heard`. Connections
  shows the services as each device's (no keys, no "broken" from the server).
- Switching to `device`: what the server has queued for the account is sent first; the keys go once it is empty. Until
  then Connections says how many are still to go. The keys are gone for good: back to `server` means signing in to
  each service again, which Connections says before the switch.
- Switching to `server`: the devices hand over what they have not sent yet (below); from then on the server's queue
  fills as 0068 has it.
- Other servers' plays, through the server: `POST /api/scrobbles` `{plays: [{id, at, artist, title, album,
  album_artist?, duration_secs?, mbid?}]}`, queued as the server's own are (one row per connected service), never
  written to the history. Its limits: at most 500 plays a call; `id` required, kept per account as 0087's ids are, so
  a play a device sends again is queued once; `at` not in the future (a few minutes' slack); a play older than 14
  days queued for ListenBrainz only (Last.fm refuses it). "Now playing" for them: `POST /api/scrobbles/now`
  `{artist, title, album, duration_secs?}`, one per account at a time, the latest winning, sent best effort, never
  queued. Both are refused while the account is `device`, or has tracking off.

## Device

- One way for every player of Cratarr's: a play is scrobbled where it is heard (0022's rule for when a play counts),
  queued with its id and its time, sent in batches, again later when a service is down. "Now playing" is best effort.
- Its sign-ins are kept in the device's own store: the app's (Android's keystore), the desktop app's, a browser's
  local storage. Never sent to Cratarr's server.
  - ListenBrainz: the user token, checked on entry (`validate-token`).
  - Last.fm: the person's own API key and secret (as now: Cratarr ships no key of its own), then Last.fm's web
    sign-in, coming back to the page; the session key traded for (`auth.getSession`) and calls signed on the device.
- Under `server`, the device sends no scrobbles of its own: its plays of Cratarr's tracks are the reports it already
  makes, its plays of other servers' tracks go to `/api/scrobbles`.
- Switched to `server` while it holds a queue: it hands the plays of other servers' tracks to `/api/scrobbles` and
  drops those of Cratarr's own (the server has counted them, and sends them itself), then stops.

## Consequences

- The private choice is the default, and it works the same for every server's tracks.
- From each device, each device needs its own sign-ins: more steps for a household with many devices. Moving a
  sign-in from one device to another without the server (a code shown on one, read by the other) is left for later.
- Two ways to send, kept apart by the one rule (the player that sounded it) and the play's id.
- A user with no Cratarr server at all (only modes of other servers) can scrobble from each device only.
- To check when building: ListenBrainz's and Last.fm's APIs answering a page in a browser (cross-origin requests); if
  one does not, the browser goes through the app's bridge where it can, and otherwise offers through the server only.

## Amendment (2026-10-31): several independent servers

With modes per server (0.13) a person has accounts on several servers. The owner chose (question set) one choice and
one switch per server, not a choice per server:

- **One choice per person:** from this device, or through one Cratarr server of theirs, the one they signed in to the
  services on. Through it, the plays of every other server's tracks go to that one (`/api/scrobbles`). A second
  Cratarr server scrobbles its own tracks only if it is the one chosen.
- **Per Jellyfin or Subsonic server, in its mode's settings: "This server scrobbles for me"** (off at the start).
  Subsonic's `scrobble` registers a play, which a server forwards to Last.fm (Navidrome, Gonic, Airsonic-Advanced: and
  to ListenBrainz) where its user set that up there; a Jellyfin does through a plugin. No API says whether a server
  does, so the person says. On, the device leaves that server's tracks to it and only reports their plays, as it
  always does (each server's own history and counts); off, the device (or the chosen Cratarr server) scrobbles them.
  A play is then never sent twice.
