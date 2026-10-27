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

**Two heads, one engine (the owner, 2026-10-27: "it is physically impossible to go forward and backward at the same
time"):** the engine owns the playhead and the speed; it sounds through two players that take turns, never together.
The track's own player for forwards at up to 1× (a browser's MSE stream, gapless and levelled as now), the wind
stream's for everything at speed or backwards. A change between them happens only at speed zero, which is silent, so it
is unheard; the one resting is kept armed at the playhead (the track buffered where the wind will land, the wind stream
asked for from where the track is as soon as a wind may come: a press on Previous or Next, a drag), so a change never
waits. Below a player's slowest speed (a browser's 1/16) the gain carries the last of a ramp down to silence. The page
reads the wind stream itself (fetch into MSE, not an audio element), so its map, and with it the playhead across cuts,
is known at every moment. Cheaper than one read head over decoded sound (no ~100 MB of decoded track on a tablet),
and the same to the ear; an AudioWorklet read head stays the fallback if a change at zero ever proves audible.

**App (after):** the same model, two heads under one engine (ExoPlayer forwards, the wind stream at speed), a5's.

## Consequences

- No more "a ramp is waited out": every special case between the two players goes.
- Memory: as now (two streaming players), nothing decoded whole.
- The page reads the wind stream itself (fetch, not an audio element), so its map is known.
- Levelling, groups (0037) and the lock screen act on the engine, so on everything it plays.
