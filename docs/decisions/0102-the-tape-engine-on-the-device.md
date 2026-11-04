# 0102: The tape engine on the device

- Status: accepted (the owner, 2026-11-03: "Why can we not have the pure engine locally too"; its three choices by
  question; for 0.16)
- Date: 2026-11-03
- Builds on: 0074 (winding), 0075 (one tape engine), 0101 (the rules crate on every device), the tape measured
  truthfully (2026-11-03: "No guessing anywhere")
- Changes: 0074's streams (`/api/stream/{source}/{id}/wind`, `…/rewind`, the brake stream): the server makes no sound
  of the tape's any more

## Context

Every wind's sound is made on the server and sent as a stream: playing backwards, Next's fast-forward, a hold, a
let-go's brake. Each was so because the players (ExoPlayer, an audio element) play only forwards, Android's output speed
goes no lower than a quarter, and a device does not always have the file.

What it cost, seen on 2026-11-03:

- **The network in every wind.** Next's fast-forward over the car's mobile data, played as the track going on
  quietly; backwards play that did nothing while staging answered slowly or the Wi-Fi was off ("clip late").
- **Two engines.** One makes the sound on the server, one steers it on the device; the guessing the owner had removed
  that day lived in the seam (a place counted on at the speed set now, a stream's rate applied late).

## Decision

**The tape engine runs on the device that plays; the server serves only the files.**

1. **The device decodes the track into memory** (PCM; about 10 MB a minute of stereo at 44.1 kHz) and plays it from
   there at any speed, either way, the pitch following: the app through its own audio chain (Varispeed, 0.9.14, on the
   decoded sound), the page through Web Audio. Backwards is reading the same sound the other way.
2. **The engine's arithmetic is the rules crate's** (0101): the envelopes, the let-go's plan, the brake, the knob, where
   a hold lands; one engine on the server's tests, in the app and in the page, measured where the sound is heard.
3. **Winding through the queue** decodes the tracks it reaches, as it reaches them (only the playing track is kept
   decoded ready), at the speed it winds (what a tape
   heard at 16× and more is pieces of it, as the server's streams were).
4. **A file kept on the device** (Sync, own music, 0101) winds with no connection at all; a streamed track winds once
   its file has come.
5. **The server's wind, rewind and brake streams go** once every player has the local engine.

## The owner's choices (2026-11-03, by question)

- **A streamed track's file is fetched whole at once** as it starts playing; a wind pressed before it has come waits
  until it can, the deck saying so.
- **Only the playing track is kept decoded:** winding into the next track waits a moment while it decodes.
- **The app and the page move together**, in one version; the server's wind streams retired at once, no mixed period.

## Consequences

- Winds start at once and never depend on the network for a track that is there; no "clip late".
- One engine, one set of numbers everywhere; the seams where guessing lived are gone.
- Memory and battery while a track is decoded; a long track decoded on an older tablet takes a moment.
- A real rewrite of both players' sound path; the server's wind code (`wind.rs`, `rewind.rs`) retired.
