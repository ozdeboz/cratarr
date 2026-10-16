# 0037: Sync play: one queue on several of Cratarr's players at once

- Status: proposed
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
