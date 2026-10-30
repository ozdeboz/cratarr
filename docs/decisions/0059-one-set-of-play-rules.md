# 0059: One set of play rules, run where the library and the queue are

- Status: accepted (the owner, 2026-10-23: "Agree, proceed"; "we also support local libraries, and for this we need
  local functions"; "how to … design this architecturally in the most robust way"; "One Rust crate"; "the local
  libraries should also be used for offline, indeed")
- Date: 2026-10-23
- Builds on: 0033 (queues; amended: shuffle is once), 0036 (radio and autoplay lanes), 0052 (local mode), 0038 (kept on
  this device), 0058 (one menu system); the amendment to 0024 (what plays is the server's choice, 500 at most)

## Context

What plays, and in what order, was decided wherever a button was pressed, from whatever that screen had loaded. A crate
of 16,000 tracks repeated the same 200. The fix moved the choice to the server (`POST /api/play`). The owner then asked
to move more there (queue edits, Play On's and voice's own play paths), so every player (the page, the app, Home
Assistant, the car, voice) follows one rule: a queue always has an order, shuffle is once, a queue holds 500 at most.
The rules sat in three places, each with its own shuffle: the server's `app/play.rs`, its Play On queue (`app/queue.rs`)
and the page's own player (`lib/localPlayer.svelte.ts`).

But Cratarr also plays a device's own music (local mode, 0052) and plays offline from what is kept on the device
(0038). Neither can ask the server. A rule only the server knows would not hold there.

## Decision

Three layers, kept apart:

1. **Where the tracks come from: a library source.** The server's library, or this device's own (local mode, and
   offline: what is kept or downloaded here). Each answers one question: a collection's tracks, in its order.
2. **The rules, pure: one Rust crate, `cratarr-rules` (`server/rules/`, a member of the server's Cargo workspace).** No
   I/O, no clock, no randomness of its own: the caller passes a seeded generator (`Rng`, xorshift64 seeded through
   SplitMix64), so a seed gives the same answer on the server and in the page. It builds for `wasm32-unknown-unknown`
   as it is. Its rules, on plain data (places in a list, a track's lane, a queue entry's key; the caller keeps the
   tracks):
   - `pick`: which of a collection's tracks a play queues, at most 500 (or less when asked): Play, Play next and Add to
     queue in its order (from a track on, those before it filling up near the end); Shuffle drawn evenly from all of it.
   - `add_place`: where tracks added by hand go (Play next right after the playing one, Add to queue after those added
     before).
   - `shuffle_order`: a shuffle, once. The playing track stays; what was added by hand stays right after it, Radio's
     inserts for it after those, Autoplay's tail last; only the queue's own tracks are mixed.
   - `upcoming_start` (a clear keeps the queue up to the playing track) and `restore_order` (the Undo of a shuffle or a
     clear: what was to come, less what played since, in place of what is to come now).
   - Shared cases: `server/rules/tests/vectors/*.json` hold inputs, seeds and answers; every build of the rules checks
     the same files. They are written again only when a rule changes on purpose.
3. **Who holds the queue: the server (its Play On players) or this device** (the page or the app playing here, its queue
   synced to the server when online).

The rules run where the library and the queue are: the server's library and queue on the server; a device's library,
or offline, on the device. The server links the crate; the page runs the same crate compiled to WebAssembly (the app's
local mode runs in the page).

## Consequences

- The server uses only the crate for these rules: every play, Play On's queue edits (`shuffle` once and `clear`, each
  answering with its Undo; 0033's amendment), a playlist's Play and the voice assistant's play by name.
- The page's own rules in TypeScript (`shuffleUpcoming` and the picking around local playback) are replaced by the
  crate's WebAssembly build once it is bound; until then they follow the vectors.
- Home Assistant's and a car's "shuffle on" is the one-time shuffle; nothing reports a shuffle state. The Android app's
  Media3 session shuffles once too (2026-10-23): a car's, a headset's or the system's shuffle sends Play On's `shuffle`
  (local mode's queue is shuffled in the app), and the session never reports shuffle on.
- A change to a rule is a change to the crate and its vectors, never to one player alone.
- A WebAssembly build step joins the dev server and the page's image build; the server's build is unchanged (the crate
  lives inside `server/`).
