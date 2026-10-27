# 0075: One tape engine: a playhead with a signed speed, anything interrupting anything

- Status: accepted as the North Star (the owner, 2026-10-27, through claude-632: "If we have a pure tape engine, then
  the North Star would be to support interruptible start-stop"; "The North Star, as I now proposed it, is the next
  version")
- Date: 2026-10-27
- Release: the version after 0.9.12 (0.9.12 ships 0074 as built: winding on the server's wind stream, holds, winding
  seeks, winding through the queue, hardware holds, effects bypassed when switching devices)
- Builds on: 0074 (winding), Start and stop (0.9.6: vinyl, fade)
- Supersedes: the owner's rule of 2026-10-26 that a Start and stop ramp under way is waited out, never dropped

## Context

0074 made winding a direction and a speed of the track's own stream, but each player still plays it on two players
in turn: the track forwards on its own (a browser's MSE stream, the app's ExoPlayer), the wind stream backwards on a
second one. Whatever runs (a ramp, a wind) is committed: play, pause, next and previous wait it out. Each new effect
has been one more special case between the two.

## Decision

**Each player has one tape engine: a playhead (track, position) with a signed speed. Every control sets a target
speed (and a curve to reach it); the engine moves there from the speed it has now. Anything interrupts anything.**

- Pause, play, a vinyl ramp, a fade, a tap's wind, a hold, its brake, a winding seek: all are targets for the speed
  (or the gain, for a fade), reached from where the engine is. A press during a ramp or a wind takes over from that
  instant: no waiting, no jump.
- The engine always knows its position, to the sample, whatever sounds; the counter, the waveform, the tape deck and
  the reports read it.
- Speed zero is silent, so a change of direction (or of what sounds) happens there, unheard.
- A condensed wind (0074's cuts) is a plan the engine follows; an interruption drops the rest of the plan and goes on
  from the real position.

**Web (first):** one read head in Web Audio (an AudioWorklet) over decoded sound, at any signed speed, as a DJ deck's.
A decoded window around the playhead (about ±30 s) serves Start and stop, short reverses and scrubbing; the server's
wind stream serves long winds, joined at a moment of zero speed (fetched ahead, so the join never waits). Borrowed
where it can be (xwax and Mixxx solved the read head; GPL-3 is compatible, decision on licences).

**App (after):** the same model, the engine writing to AudioTrack with its own speed and resampling (ExoPlayer plays
forwards only); a5's.

## Consequences

- No more "a ramp is waited out": every special case between the two players goes.
- Memory: a decoded window, not whole tracks (a 5-minute track decoded is ~100 MB, too much for the tablet).
- The page reads the wind stream itself (fetch, not an audio element), so its map is known.
- Levelling, groups (0037) and the lock screen act on the engine, so on everything it plays.
