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
