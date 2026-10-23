# 0059: One set of play rules, run where the library and the queue are

- Status: accepted (the owner, 2026-10-23: "Agree, proceed"; "we also support local libraries, and for this we need
  local functions"; "how to … design this architecturally in the most robust way"; "One Rust crate"; "the local
  libraries should also be used for offline, indeed")
- Date: 2026-10-23
- Builds on: 0033 (queues), 0036 (radio and autoplay lanes), 0052 (local mode), 0038 (kept on this device), 0058 (one
  menu system); the amendment to 0024 (what plays is the server's choice, 500 at most)

## Context

What plays, and in what order, was decided wherever a button was pressed, from whatever that screen had loaded. A crate
of 16,000 tracks repeated the same 200. The fix moved the choice to the server (`POST /api/play`). The owner then asked
to move more there (queue edits, Play On's and voice's own play paths), so every player (the page, the app, Home
Assistant, the car, voice) follows one rule: a queue always has an order, shuffle is once, a queue holds 500 at most.

But Cratarr also plays a device's own music (local mode, 0052) and plays offline from what is kept on the device
(0038). Neither can ask the server. A rule only the server knows would not hold there.

## Decision

Three layers, kept apart:

1. **Where the tracks come from: a library source.** The server's library, or this device's own (local mode, and
   offline: what is kept or downloaded here). Each answers one question: a collection's tracks, in its order.
2. **The rules, pure: one Rust crate, `cratarr-rules` (`rules/`).** No I/O, no clock; randomness from a seeded generator
   the caller passes. Picking at most 500 from a collection (in order, from a track, a fair shuffle of the whole);
   shuffling the upcoming tracks once (what was added by hand right after the playing track, Radio's inserts after
   those, Autoplay's tail last, only the queue's own tracks mixed); clear and its Undo; play next and add. Shared test
   vectors (input, seed, expected output) pin the behaviour.
3. **Who holds the queue: the server (its Play On players) or this device** (the page or the app playing here, its queue
   synced to the server when online).

The rules run where the library and the queue are: the server's library and queue on the server; a device's library,
or offline, on the device. The server links the crate; the page runs the same crate compiled to WebAssembly (the app's
local mode runs in the page). There is one implementation; the vectors check both builds.

## Consequences

- The page's own rules in TypeScript (`shuffleUpcoming` and the picking around local playback) are replaced by the
  crate's WebAssembly build once it is bound; until then they follow the vectors.
- Every surface that starts playback on the server's library goes through `/api/play`; the device's library goes
  through the same rules on the device.
- Home Assistant's and a car's "shuffle on" is the one-time shuffle; nothing reports a shuffle state.
- A WebAssembly build step joins the dev server and the image build.
