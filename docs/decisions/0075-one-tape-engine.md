# 0075: One tape engine: a playhead with a signed speed, anything interrupting anything

- Status: accepted as the North Star (the owner, 2026-10-27: "If we have a pure tape engine, then
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
waits. Forwards and backwards are different things at the core (the owner), and the split follows that; winding
forwards at speed rides the wind stream too, as the server renders speed without the browser's aliasing, but a wind
always begins from a pause and ends braked to a stop, so every change still happens at zero. Below a player's slowest speed (a browser's 1/16) the gain carries the last of a ramp down to silence. The page
reads the wind stream itself (fetch into MSE, not an audio element), so its map, and with it the playhead across cuts,
is known at every moment. Cheaper than one read head over decoded sound (no ~100 MB of decoded track on a tablet),
and the same to the ear; an AudioWorklet read head stays the fallback if a change at zero ever proves audible.

**App (after):** the same model, two heads under one engine (ExoPlayer forwards, the wind stream at speed).

## Consequences

- No more "a ramp is waited out": every special case between the two players goes.
- Memory: as now (two streaming players), nothing decoded whole.
- The page reads the wind stream itself (fetch, not an audio element), so its map is known.
- Levelling, groups (0037) and the lock screen act on the engine, so on everything it plays.

## The app's engine (2026-10-28)

The owner, on the tablet (0.9.14-1): the waveform and the tape deck moved in strides through every wind, reverse play at
1× included; a hold wound to the queue's end landed past its last track's end and the page counted on; "These symptoms
are of an impure machine". Asked where the machine was not pure, the owner chose all three: the player's course logged,
the curve over the bridge, and this engine in the app.

- **One truth, `StartStop.deck()`** (`Deck`): the queue's track, the place in it, the speed (track ms per ms, signed) and
  how fast it changes (per second), from the head that sounds: the wind's stream through its map, or the track's player.
  The page, Cratarr's report and the session all read it. Start and stop's held place is the one lie; the tape's own
  stops and starts (and a play taking over a pause) show the truth.
- **The curve over the bridge:** the `player` report carries `speedX` and `accelX`; the page draws each frame from
  place + v·t + ½·a·t², told afresh every 100 ms while the deck moves other than playing on as it is.
- **Landings by the files as they are:** a wind lands by its map (Cratarr's lengths), counted from the track it began
  in, then settled on the player's own lengths (`Deck.settle`); wound forwards to the queue's end it rests there,
  stopped.
- **Anything interrupts anything:** a pause during any wind brakes it from its rate and rests where it got; a press the
  other way during a ramp turns it from the level it has reached (`turn`); nothing waits for a ramp's end.

## Spike B: the rules crate in the Android app (2026-10-30, 0.11)

The brake (`server/rules`'s `brake`) reached the app through UniFFI 0.29 (`server/rules-ffi`, its own workspace so the
server and the page's WebAssembly stay without it), built in Docker by `scripts/rules-android.sh` (cargo-ndk, NDK r27c;
the library is built, never kept in the repository). The device's `rustRules` switch, off by default, puts it under the
tape's let-go plan (`Plan`); switched on, `RustRules` compares it with the app's Kotlin and logs what it found.

Measured on the x86_64 emulator, a debug build (so both sides slower than a release build on a phone; the ratio is the
point):

- **The same answers:** 4,272 calls over 16 brakes like the tape's (speeds −128× to 128×, to 1× and to still), the
  greatest difference from the Kotlin 0, bit for bit.
- **Loading:** the first call 120 to 140 ms (JNA finds and binds the library); every later call is warm.
- **A warm call:** about 100 µs through UniFFI and JNA, against 2 to 5 µs for the same sum in Kotlin. A frame that asks
  the brake three times spends about 0.3 ms.
- **Size:** the debug APK 1.6 MB larger (21.96 → 23.59 MB): the library 320 KB (arm64) and 350 KB (x86_64) at `opt-level = "z"`, and JNA's own
  native part for every ABI it ships (trimmed to the app's).

So the shared Rust works in the app and agrees with the page and the server, but a call costs too much to be made per
value. The one tape engine (0075) crosses into Rust **once per frame or per audio buffer**, its loop inside Rust,
handing back the whole frame's answers: never per sample, never per value. If even that is too dear on a phone, the
hot path takes direct JNI (the `jni` crate) and UniFFI keeps the rest.
