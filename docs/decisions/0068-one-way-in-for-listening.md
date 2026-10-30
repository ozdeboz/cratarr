# 0068: One way in for listening: written first, then told

- Status: accepted (the owner, 2026-10-24: "our event architecture is somewhat degraded… possibly we should do a tiny
  bit of refactoring to go a long way in keeping things up and aligned with standard best practices"; "Yes, make it
  align with standard best practices")
- Date: 2026-10-24
- Builds on: 0011 (the journal), 0022 (when a play counts), 0033 (Cratarr holds the queues), 0064 (the listening
  history)
- Serves first: scrobbling to ListenBrainz and Last.fm (Connections, the owner the same day)

## Context

Plays were written in four places, each its own way: the web player and the app (`serve::queues::record`), Subsonic
apps' scrobble (`serve::routes`, `count_play` directly), and Cratarr's own source (`source::cratarr`, again directly);
imports apart. "A track started" lived in four more, none joined: Play On's reports, Subsonic's now-playing sessions,
the page's own player (a Play On player too), Kodi (polled into Play On). Cratarr had two event systems, neither for
listening: the journal (durable events of library actions, to undo and resume, 0011) and the page's live channel (what
changed, for open pages). Anything reacting to listening (scrobbling first) had to hook into several places.

## Decision

**One way in, `app::listening`:**
- `heard(user, song, how)`: a track heard (0064's `Heard`). Written first, in one transaction: the history's row, the
  counts when it counts (0022), and a row in each connected service's send queue (a transactional outbox). Then told.
- `started(user, player, song)`: a track begins on a player. Nothing kept; told, once per change of song on that
  player.
- Every live play goes through them: the web player and the app (`/api/plays`), Subsonic apps (`scrobble`,
  `reportPlayback`), Cratarr's own source, Play On's reports (which is also how the page's player and Kodi report).
  Imports are not live listening: they write the history themselves and tell nothing (0067).

**Told on one in-process channel** (publish and subscribe; `tokio::sync::broadcast`): `Started` and `Heard`. A
subscriber that falls behind skips what it missed rather than holding anyone up; what must not be lost is already
written before anything is told. The first subscriber is the scrobbler (now playing on `Started`, a wake-up on a
counted `Heard`); the history's views and Home Assistant's now-playing can follow without each polling.

**The journal stays the journal:** durable library actions. The channel is for live listening signals. Each has one job.

## Consequences

- `serve::queues::record`, `count_play` and the direct `userdata.count_play` calls in `serve::routes` and
  `source::cratarr` call `listening::heard`; `UserData::record` stays the one write (imports call it directly).
- Play On and Subsonic's sessions call `listening::started` when what a player plays changes.
- Scrobbling: the send queue is filled in `heard`'s transaction (so a play is never lost between writing and sending),
  and the sender wakes on the channel.

## Amendment (2026-10-25): the way in closes when an account keeps no history (0.9.5)

With an account's `settings.tracking` off (Personal › Tracking, see 0064's amendment), the way in takes nothing for it:

- `heard` writes nothing and tells nothing, so no scrobble is queued.
- `started` tells nothing, so there is no "now playing".
- `UserData::record` refuses the play for every other writer too, including the imports. The listening imports
  (ListenBrainz, Last.fm's scrobbles, history files) are refused with "Play history is off for this account".
  Last.fm's loved tracks are favourites and still come in.
- The scrobbler drops whatever was queued for the account before the switch.
