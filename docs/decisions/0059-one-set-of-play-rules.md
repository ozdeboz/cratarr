# 0059: One set of rules for what plays and in what order, shared by every player

- Status: accepted (the owner, 2026-10-23, relayed by the coordinating session)
- Date: 2026-10-23
- Builds on: 0024 (amended 2026-10-23: what plays is the server's choice), 0033 (Cratarr holds the queues), 0036
  (Radio's and Autoplay's lanes). Amends 0033 (shuffle is once).

## Context

What plays, and in what order, is the server's choice (0024, amended 2026-10-23): at most 500 tracks a queue, picked
from the whole collection. A queue always has an order: shuffle is a one-time reorder, and Undo puts the order before
back. The rules sat in three places: the server's `app/play.rs`, its Play On queue (`app/queue.rs`), and the page's
own player (`lib/localPlayer.svelte.ts`), each with its own shuffle. Players must keep them offline and on a device's
own library too, where no server answers.

## Decision

- **One implementation, a small pure Rust crate**, `cratarr-rules` (`server/rules/`, a member of the server's Cargo
  workspace): no I/O, no clock, no randomness of its own. The caller passes a seeded generator (`Rng`, xorshift64
  seeded through SplitMix64), so a seed gives the same answer on the server, in the page (WebAssembly) and anywhere
  else. It builds for `wasm32-unknown-unknown` as it is; its WebAssembly binding is the page's.
- **Its rules, on plain data** (places in a list, a track's lane, a queue entry's key; the caller keeps the tracks):
  - `pick`: which of a collection's tracks a play queues, at most 500 (or less when asked): Play, Play next and Add to
    queue in its order (from a track on, those before it filling up near the end); Shuffle drawn evenly from all of
    it.
  - `add_place`: where tracks added by hand go (Play next right after the playing one, Add to queue after those added
    before).
  - `shuffle_order`: a shuffle, once. The playing track stays; what was added by hand stays right after it, Radio's
    inserts for it after those, Autoplay's tail last; only the queue's own tracks are mixed.
  - `upcoming_start` (a clear keeps the queue up to the playing track) and `restore_order` (the Undo of a shuffle or a
    clear: what was to come, less what played since, in place of what is to come now).
- **Shared cases**: `server/rules/tests/vectors/*.json` hold inputs, seeds and answers. The crate's tests check them;
  every other build of the rules checks the same files. They are written again only when a rule changes on purpose.
- **The server uses only the crate** for these rules: every play (0024's amendment of 2026-10-23, and below), Play On's
  queue edits, a playlist's Play and the voice assistant's play by name.

## Consequences

- The page's own shuffle and clear move to the crate's WebAssembly build (the page's session); until then it keeps
  its TypeScript copy, which follows the same rules.
- A change to a rule is a change to the crate and its vectors, never to one player alone.
- The Dockerfile is unchanged: the crate lives inside `server/`, which the image's build copies whole.
