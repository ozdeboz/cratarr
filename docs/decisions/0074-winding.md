# 0074: Winding: the track heard backwards and forwards at speed, as a tape deck winds

- Status: accepted (the owner, 2026-10-27: reverse play as close to normal play as possible; one
  standard way to shorten long audio, cutting and crossfading; Start and stop's vinyl and fade in reverse too; holding
  Previous and Next as a deck's REW and FF; seeking may wind; one set of controls; 12 s at most for a tap, a hold as
  long as it is held; the queue as one tape where chosen; other players and groups; hardware long presses; quieter
  while winding; no wind counts as listening)
- Date: 2026-10-27
- Release: after 0.9.11 (whose timing found the server fast: the whole clip taken by a player within a second)
- Builds on: 0.9.9's Tape rewind (a clip on a player of its own), Start and stop (0.9.6: vinyl, fade), 0033 (Play On),
  0037 (sync groups), 0064 (what a play heard)

## Context

Tape rewind (0.9.9, tuned in 0.9.10) is a clip the server renders, played by each player on a second player of its
own, then the track from 0:00. It works, but it is a thing apart: no levelling, no group, no lock screen, and Start
and stop cannot act on it. Its own spin-up and brake are rendered in, so a tap and a hold could not sound alike, and
the owner wants holds, forward winding and winding seeks too.

Limits that shape it: a browser's `playbackRate` stops at 16× and resamples without a real anti-alias; Android's
AudioTrack speed stops at 8×. Slowing a stream down (rate below 1) aliases nothing.

## Decision

**Winding is a direction and a speed of the track's own stream. The server renders the speed (anti-aliased, its
half-band pyramid); each player adds the envelope, its own vinyl ramp, never above 1×.**

- **The stream:** the track from X, backwards or forwards, at a constant speed v (the sustain level, 2–16×):
  open-ended (until the player stops reading, or the edge), or condensed to a length (a tap, a winding seek). Quieter
  than the music by the device's wind volume (`gain_db`, −12 dB out of the box), rendered in.
- **The envelope (ADSR on speed):** attack = the player's vinyl ramp from still to 1 over `attack_ms`, sustain = the
  stream as it comes, release = the vinyl ramp from 1 to still over `release_ms` (a deck brakes faster than it spins
  up). A tap shorter than attack + release peaks lower: the release starts from wherever the attack had got to. The
  clip time a player consumes is c = ∫ rate dt; the counters follow the track's place at c.
- **Condense** (a module of its own, for previews and teasers too): a span from X to T, backwards or forwards, at v,
  fitted to a wall length D with the envelope above: the clip time is C = ∫ rate over D (raised-cosine ramps: each
  consumes half its length). Where v·C covers less than |T − X|, the middle is cut into pieces of `piece_ms` spread
  evenly over the span, each jump crossfaded (15 ms, equal power), the first piece continuing from X and the last
  ending on T; where it covers more, v is lowered until it fits. Never faster than asked, never cut when it need not
  be.
- **What the owner sets (per device; the page's, the app takes them):** duration as seconds per minute wound (0.5–2;
  D = ratio × |T − X|/60, a tap and a seek at most 12 s), speed limit (2–16×), spin-up (attack), brake (release),
  piece length, wind volume, "seeking winds the tape" (on or off), and at the track's edge: stop, or wind on through
  the queue.
- **Gestures:**
  - Previous tapped: winds back to 0:00, condensed (early in the track, the previous track as ever). Next tapped: the
    next track as ever.
  - Previous or Next held: pauses with Start and stop (vinyl wind-down, fade, or at once), then winds back or forward
    (attack), as long as held (sustain), at most to the edge (or on through the queue); on release brakes (release),
    then plays on from there with Start and stop's wind-up.
  - A seek, where "seeking winds the tape" is on: winds from X to the target T, condensed, landing exactly on T, then
    plays on; off, a jump as ever. Dragging the waveform winds once let go.
- **The queue as one tape** (where chosen): an open-ended wind past the edge carries on into the neighbouring track;
  the stream is given the tracks in its way and its map says where one ends.
- **Other players and groups:** a hold drives any Cratarr player: `wind` with start and stop, the player doing the
  envelope itself; a group's followers hear the leader's sound as ever (0037).
- **Hardware:** the lock screen's, a headset's and a car's long press of previous and next are holds, where Media3
  passes them on.
- **No wind counts as listening**, backwards or forwards, tap, hold or seek: its time adds nothing to the play's heard
  time (0064), and the place it reaches is not heard either.
- **Fallbacks:** where nothing can be rendered (a Bandcamp preview, the app's device music, an offline copy without
  the server), a plain jump.
- **How the players play it:** the web appends it to its one Media Source stream as a segment (its own timestamp
  offset); the app swaps the playing item's source for [wind, track from where it lands] without touching the queue
  (a spike first; until it lands, the app keeps its small player of its own for the wind, with the same envelope).
  Levelling, output, groups, ducking, the visuals' tap and the lock screen then act on it as on any music.

## Endpoints

- `GET /api/stream/{source}/{id}/wind?from_ms=X&dir=back|forward&speed=v&gain_db=G` — open-ended: chunked, until
  the player stops reading or the edge.
  - Condensed: `&to_ms=T&fit_ms=D&attack_ms=A&release_ms=R&piece_ms=P` (a tap back: T = 0).
  - Through the queue: `&through=source/id,source/id…` (the tracks in its way, nearest first).
  - `&as=wav|flac|opus` as the forward streams' forms (WAV the plain one; FLAC and Opus in fragmented MP4).
  - Headers: `x-wind-speed`, `x-wind-dir`; condensed, `x-wind-map`: knots `[[clip_ms, track, track_ms], …]`, linear
    between, a cut two knots at one `clip_ms`; the counters' smooth pace is the straight line from X to T in c.
  - 404 where it cannot be rendered (the player jumps); 503 while another decode holds the turn past a moment.
- A hold on another player: `POST /api/players/{name}/command` `{ "command": "wind", "dir": "back"|"forward",
  "phase": "start"|"stop" }` (Play On's commands, as Next and Previous).
- The device settings travel with Start and stop's (`startStop.wind`: `ratio`, `speed`, `attackMs`, `releaseMs`,
  `pieceMs`, `gainDb`, `seekWinds`, `edges`: `"stop"|"queue"`), replacing 0.9.10's four Tape rewind controls.
- `GET /api/stream/{source}/{id}/rewind` stays for players before this, its shape held as 0.9.10's.

## Consequences

- One renderer, one envelope, one way of shortening: tap, hold and seek sound alike on every player.
- The players do less: no second player in the end, their own ramp on a stream they already know how to play.
- The app's swap of the playing item's source is the risky part; the small player stays until it is proven.
- Counting rules gain one: winding is never heard.
