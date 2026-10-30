# 0037: Sync play: one queue on several of Cratarr's players at once

- Status: accepted (built 2026-10-16 over Snapcast, 0bc3ade0 and on; marked at the owner's word, 2026-10-19)
- Date: 2026-10-16
- Builds on: 0031 (the Android app and Play On), 0033 (Cratarr holds the queues)

## Context

The device sheet (Play on) chooses one output: a tap moves the music there. The owner (2026-10-16): a long press on a
device that is not played to should start **sync play**, several devices playing the same queue.

Decided with the owner the same day:

- **Same-room tight**: within about 20 ms of each other, so two devices in earshot sound as one. Looser sync
  (starting together, then jumping back into line when they drift) would leave an audible echo wherever two devices
  are heard at once.
- **Only Cratarr's app players** join: the Android app, which Play On already reaches reliably. A page in a browser
  pauses when its screen sleeps, and its output delay is less predictable (0031: it is not a player).

## Decision

**A group is one queue (0033) played by several players.**

- The queue is the one played to when the group formed (the leader's). The others follow it and keep no queue of
  their own while grouped; leaving, a player keeps nothing of the group's.
- A command to any player of a group is the group's: play, pause, next, previous, seek, the queue's edits, the
  sleep timer. Volume stays each player's own.
- A player that goes away (offline, its lease over: 0031) leaves the group, and the group plays on. Coming back, it
  does not join again by itself.
- The leader leaving hands the queue to the longest-joined player that stays, as a handover (0033) without a stop.

**The page.**

- In the device sheet, a long press on one of Cratarr's players that is not played to adds it to the group of
  the one played to. The grouped players show ticked; a long press on a grouped one takes it out.
- A tap plays on that one player alone, as now: the group ends.
- The long press is offered only while the output is one of Cratarr's players. With this device or the server's
  player chosen, there is no group to join, and the long-press label says so.
- The device button and the sheet's chip name the group ("Salon tablet + Phone").

**Keeping time** (as Snapcast does, on what the app already plays with):

- **One clock.** Each player works out how far its clock is from Cratarr's, over the live channel, all the time:
  a timed ping, where the shortest round trip of the last ones wins. Everything below is in Cratarr's time.
- **Scheduled, not immediate.** Cratarr says "entry E, at position P, sounds at time T", with T a little ahead
  (enough for every player to have the track ready). Each player prepares it and starts so that its first sample
  leaves the speaker at T, allowing for its own output delay (Media3 reads it from the audio track's timestamps).
  Pause, seek and a jump are scheduled the same way. Track changes need nothing: each player plays the same queue
  gaplessly.
- **Held in line.** Each player compares where it is (the frame being heard, at a time) with where it should be.
  A small difference is taken out without a jump: a few samples dropped or repeated, or the tempo a hair off for
  a moment, in an audio processor in the player. A large one (over about 50 ms, after a stall) is a scheduled seek.
- **The same audio.** Every player of a group streams the same file the same way. A device that would get a
  different format (an Opus copy, 0033's downloads) plays the original while grouped, since decoders start with
  different delays.

**Home Assistant.** Each player in the push says its group (the leader's name). Join and leave are Play On commands,
so HA's `media_player.join` and `unjoin` map to them.

## Steps

1. The clock: each player's offset to Cratarr's, and how far apart two players are when both play the same track.
   This is measured first, without groups, to know what the devices manage.
2. Groups at Cratarr: join, leave, one queue, commands to the group, the push.
3. The scheduled start, pause, seek and jump.
4. Holding in line: the audio processor, measured against step 1's figure on the tablet and the phone side by side.
5. The page: the long press in the device sheet, the group in it and on the device button.
6. HA's join and unjoin.

## Consequences

- Groups live at Cratarr, as queues do (0033), so any page or HA sees and changes the same group.
- The app gets an always-running clock exchange and an audio processor of its own; plain playback is unchanged
  outside a group.
- 20 ms is a target to measure, not a promise: Wi-Fi stalls longer than the buffer still break the sound, and the
  group catches up after.

## Prior art: Jellyfin's SyncPlay (read in its source, 2026-10-16)

Jellyfin's SyncPlay (jellyfin-web `src/plugins/syncPlay`, server `Emby.Server.Implementations/SyncPlay`) is made for
watch parties in different places, not for devices in one room.

- **Clock:** NTP's four timestamps. The client sends, the server stamps the request's arrival and the answer's
  departure (`GET /GetUtcTime`), and the client stamps the arrival. Of the last 8 pings the shortest round trip wins.
  It pings every second three times, then once a minute, on the wall clock with millisecond `Date`s.
- **Commands:** every change goes through a Waiting state. The server waits until each member says it is ready
  (buffered), then broadcasts unpause with `When` (now plus the highest ping, 500 ms by default) and a position. Each
  client sets a timer to its local equivalent of `When`. A member counts as ready within 500 ms of the position.
- **Holding in line:** `SpeedToSync` sets the playback rate to 1 + difference/1 s for one second when 60 ms <
  |difference| < 3 s. `SkipToSync` seeks above 400 ms, checked at most every 1.5 s. Correction is **off by
  default** (`enableSyncCorrection: false`). The code's own notes say "SpeedToSync sounds bad on songs" and that both
  "have a hard time keeping up on Android Chrome".
- **Output delay:** not measured. Each device has a hand-set `extraTimeOffset`.

What Cratarr takes from it:

- the four-timestamp ping and the shortest round trip (the server stamping arrival and departure);
- the Waiting state: nobody starts before everyone has the track ready, then a scheduled start a little ahead.

What it does differently:

- a pitch-free correction in samples (Snapcast's), not a playback rate, which Jellyfin itself finds bad on music;
- the clock on the monotonic clock, and pinged more often;
- each device's output delay measured once with a microphone (a click track recorded from both), where Jellyfin
  leaves it to a hand-set offset. Jellyfin's tolerances (60, 400 and 500 ms) are an order wider than the 20 ms here.

## Measured (2026-10-16, step 1), and the choice made after the research

The clock exchange on the two devices, with nothing played or changed (preview 29):

| Device | Best round trip | Middle round trip | Offset spread (better half) |
|---|---|---|---|
| Salon tablet | 2.5 ms | 4 ms | 0.6 ms |
| Phone | 4.6 ms | 19–21 ms | 3.5 ms |

Both know Cratarr's clock well inside 20 ms. The phone's middle round trip shows Wi-Fi power saving, as the research
warned. The NAS's own clock is about 1.1 s off the devices', which does not matter: everything is in Cratarr's time.

**The owner chose Snapcast, folded in** (2026-10-16), after research into how the proven systems do it (Snapcast,
AirPlay 2, Google Cast groups, Squeezebox, and the Android studies): one stream, decoded once and stamped with the
time each chunk should sound, played out by each device with sample-level correction. This replaces "Keeping time" and
"The same audio" above.

- **Cratarr speaks Snapcast's server protocol** (port 1704; the documented binary protocol): the clock exchange,
  hello, each device's settings (volume, speaker delay, buffer), the stream's format header, and the time-stamped
  chunks. There is no separate snapserver.
- **A group's queue is decoded once at Cratarr**, gapless, in one format, and sent to its players.
- **The app plays a group with Snapcast's own player** (snapclient, as Snapdroid bundles it), unchanged. The part
  that must be exact (each chunk played at its time, samples dropped or repeated) stays Snapcast's tested code.
- **The cost:** a grouped device plays from the NAS live, with no offline playback, and a pause or skip is heard after
  the buffer (about a second). Playing alone is unchanged.
- **Kept from above:** groups at Cratarr, commands to the group, each device's speaker delay measured once with a
  microphone, and Wi-Fi held in low-latency mode while grouped.

## Amended (2026-10-16): the leader's app decodes, Cratarr times and sends

Built folded in, with one change to the above: **the group's sound is decoded by its leader's app, not at Cratarr.**
The leader plays the queue exactly as it plays alone (Radio, Autoplay, the sleep timer, repeat, downloads, the lock
screen, counting plays), its own speaker silent, and sends what its player plays (after decoding and gapless trimming,
16-bit PCM) to Cratarr on Snapcast's port. Cratarr stamps it with the time it was played, on its own steady clock, and
sends it to every device of the group, the leader's own Snapcast player too. Decoding at Cratarr would have meant a
second player there, with its own queue handling, Radio, Autoplay and sleep timer, and a remote media session on every
device; this keeps one player, the app's.

- **A break** (a seek, a jump, a pause, a stop) silences the group at once and drops the sound still to come; the new
  sound is heard as it starts playing, a buffer (1 s) later. Pausing, the leader goes back a buffer, to where the group
  was heard, so it goes on from there.
- **A new format** (a track at another sample rate) waits until the old sound has played everywhere: a short pause
  between those tracks.
- **The leader's reports** say where the group is heard (a buffer behind its player).
- **Followers** keep no queue (as above) and stay in front with a notification naming the leader. Playing something on
  a follower takes it out of the group.
- **Snapcast's player** is Snapdroid's build of snapclient (GPL-3.0), taken from Snapdroid's release at build time and
  checked against a pinned checksum, and run by the app as Snapdroid runs it; no binary is kept in this repository.
- **The page:** the group's leader shows a crown in the device sheet (the owner).
- **Port:** 1704 (`CRATARR_SNAPCAST_BIND`, empty to switch sync play off), on the host the player reaches Cratarr on,
  or on `CRATARR_SNAPCAST_HOST` when that is another: a page on a public name (behind a reverse proxy) with the port
  open on the local network only. Found on the first try: the phone, on the public name, could not reach 1704. Tested against snapclient 0.31: chunks played
  at age 0, silence on a break, heard again as the new sound began.

## Amended (2026-10-16): how the group and its settings are shown, after looking at the others

Read in their source (Snapweb, Snapdroid, Iris, Home Assistant, Music Assistant, Lyrion and Material Skin, Jellyfin
SyncPlay, moOde, Volumio): grouping is a checklist started from the player played on; a group is named after its
players; each device keeps its own volume, with a group slider (the average, moving each in proportion) once there are
two; exactly one timing setting per device, in ms, in that device's settings, never on the group's; the buffer and
drift correction are global and hidden. None measures with a microphone. The owner chose, from that:

- **The device sheet's + and tick** add a player to the group or take it out (a long press too); a tap on its name
  still plays there alone. The leader keeps its crown.
- **Volumes in the sheet**, while grouped: a group slider (the average; moved, each device goes in proportion from
  where it was, up toward 100 or down toward 0) and each device's own.
- **Speaker delay**, in Settings → Players, for any player from any device: how late that speaker sounds, 0 to 500 ms
  in steps of 5, heard at once while the finger moves. It is Snapcast's per-client latency: that device plays that
  much earlier. Kept in `players.json`.
- The buffer (1 s) and Snapcast's own drift correction stay out of sight.

## Amended (2026-10-18): the group's visuals

The owner: "the visualizations should be shared between the sync play group so that controlling it in one place
controls it in another." The group shares whatever shapes the picture (the look and its pins, colours, Intensity,
Flash, Speed, the grain, the fluid, the routes, the bands' times: the page's list, decision 0046); what a device can
afford (Points, Steps, Resolution, FPS) and how bright its screen should be stay its own. Built in preview 63.

- **Kept by Cratarr, per group** (by its leader), in memory: dropped when no player is in the group any more, gone with
  a restart (the group's devices start it again). `GET /api/groups/{leader}/visuals`: `{ version, settings }`, or 404
  until a device has started them.
- **And per player on its own** (by its name; preview 64): the owner's phone drives the salon tablet without being in
  a group with it ("my phone is most definitely controlling my salon tablet"), and its visuals follow the tablet's: a
  player's own page and every page driving it share them, as a group's devices do. A page uses its player's group
  where it has one, else the player.
- **A change:** `PUT /api/groups/{leader}/visuals` with `{ base, settings, writer }`. Its settings are merged into the
  group's: top-level keys replace, `routes` replace key by key, so two people moving different sliders at once do not
  undo each other. `base` 0 starts the group's visuals only where it has none (409 otherwise: two devices joining an
  empty group at once, and the second takes the first's); any other base, the last change wins. 404 for a group no
  player is in; 413 beyond 64 KB, or a writer id beyond 64 characters.
- **Versions** start from the clock (milliseconds) and only rise, even across a restart, so a page can ignore any it
  has seen.
- **Heard** on the players' live channel (`/api/events/players`) as `visuals`: `{ group, version, settings, writer }`,
  the group's whole settings each time (a page that missed one is right again with the next) and the page that wrote
  it (`writer`, the page's own id: it skips its own echo). A channel that fell behind gets every group's as it is now.
- **Joining** a group adopts its visuals (an empty group takes the joiner's); leaving keeps what was showing.

## Amended (2026-10-23): a page in a browser follows

The owner wanted the ThinkBook's page in the Salon tablet's group. A page now follows a group the way a Kodi does,
and never leads one: its sound cannot be sent on.

- **Its sound:** on a `group` command naming another leader, its own music stops and it reads the group's sound as
  stamped records (`/api/groups/{leader}/listen.pcm?player=`, `Snapcast::frames`: its format, then each chunk as it
  comes with its stamp on Cratarr's time, and a break's silence). It plays each at its moment on the browser's audio
  clock: the stamp, plus the group's buffer, less its speaker delay, on Cratarr's time from a clock exchange with
  `/api/time`, at the moment sound leaves the speaker (`getOutputTimestamp`) (`lib/groupListen`). Pieces within 5 ms
  of each other are laid end to end; further apart, the stamp's moment wins. Lost, it opens it again after 2 s; a
  browser that wants a tap first shows Tap to play.
- **Not the Kodi's WAV:** the first build played the Kodi's WAV in an `<audio>` element. Chrome held about 3 s before
  playing, more than the group's 1 s, so it was always late; each skip to bring it back sent nothing for as long, the
  page ran dry and rebuffered, late again: a dropout every 10 s (the owner heard them; the server's log showed "put back
  in step … 3126 ms late" every 10 s). The keep-in-step reading stays the Kodi's
  (`snapcast::listen::Listener::heard`).
- **Its log:** what it heard unevenly goes to its player log, sent every 30 s while connected: the group's sound
  opened (format, buffer, delay, the browser's latencies), its clock (offset, round trip), every 10 s a tally when
  anything went unevenly (pieces too late to play, gaps and their length, resyncs), lost and reopened, held back for a
  tap, closed. Cratarr's own log says when a page starts and stops listening (and why), and when it fell behind.
- **Its screen:** shows the group's, as an app following does; its own player reports no queue, playing while the
  group's sound plays. Its row in the Play On sheet has the group's + and tick, like any follower's.
- The caution above still holds: a page's output delay is less predictable than the app's, and a sleeping screen may
  pause it. The speaker delay setting is there for the first.

## Amended (2026-10-23): how far the followers drift, measured before correcting

The owner heard a follower fall out of step and asked how others catch up. What the Kodi's stream and the page do now
(a skip or pad of the whole difference beyond 20 ms; a jump beyond 5 ms) lets the error grow first. Read in their
sources, the proven ones correct early and gently:

- **Snapcast** (snapclient, `client/stream.cpp`, the apps' own): from 0.1 ms off it drops or repeats single frames
  (copied, not interpolated), at most 0.05% (one frame in 2,000), reached at 1 ms off; a jump only when the median
  over its last readings passes 2 ms (500 readings), 5 ms (100) or 50 ms (20), or 500 ms at once.
- **shairport-sync** (AirPlay): lets 2 ms of drift be, resyncs beyond 50 ms; simply adding or taking out frames is
  "Not recommended": it resamples with SoX where the processor allows.
- **Jellyfin's SyncPlay**: a playback rate of 1 + difference/1 s, off by default (above).

A rate of 0.5%, first thought of here, is 8.6 cents: against another speaker in the room it beats at about 2 Hz on
an A. A time-stretch that keeps the pitch smears transients and is not in Web Audio. So the correction will be
Snapcast's kind, at most 0.05%, resampled where it can be.

First, measured: once a minute the Kodi's stream (`syncplay` "drift", from its readings) and a page ("group sound
drift" in its log, from each piece laid end to end against its stamp) write down how late they played (median,
least, most, readings) and how often they were put back in step.
