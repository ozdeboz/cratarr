# 0108: A request loop never reaches the server, and a new one fails the tests

- Status: accepted (the owner, 2026-11-19: "that needs to be fixed durably and robustly"); 0.17
- Date: 2026-11-19
- Builds on: the page's one request core (`lib/api`), Svelte 5's effects

## Context

A record page's effect read a `$state` value while writing it anew, so it ran again and again. Each run asked the
server twice. The server logged over 5,000 asks a minute, slowed for everyone, and failed cover requests that the page
then remembered as "no cover" until a reload. A review then found a second such loop, a "Sounds like" crate asking for
itself, present since 0.16.

## Decision

1. **Every GET the page sends passes one guard** (`lib/loopGuard`, LG01). The same address sent more than 60 times in
   10 s is held: the 61st fails at once, said once in the console with its address. Only asks that went out count, so a
   held address goes again once they leave the window, but an address asked again within half a second while held stays
   held, because a loop never pauses. Nothing legitimate comes near it: a poll asks at most every 2 s, and a view's reads
   on the server's changes are a second apart at least, with one more after the last change (`lib/dataChanges`); writes
   are never held.
2. **A page test scans every effect** (`tests/effect-loops.test.ts`) for one that reads and writes the same `$state`,
   comments and strings left out. A new one fails the tests unless it is reviewed and listed with why it settles: its
   write is guarded (by a check of the value, or only while it holds), it is written only in a handler, an answer's
   callback or a timer, a local of the same name shadows the state, or it is no state at all (a DOM property). A listed
   one no longer found fails too, so the list stays true.
3. **A failure is never "nothing".** A server error answers as an error, not as "no cover", and the page asks again
   later. Only a real "none" is remembered.

## Consequences

- A loop shows up as a held request and a console line, not as a slow server.
- Writing an effect that both reads and writes state needs a reason, written in the list.
- Neither is a whole guard, and both are kept:
  - LG01 holds no slow loop: one under 6 asks a second (60 in 10 s) is never held, as three views reading one address at
    once is as fast.
  - The scan does not see a write through a call or through another module's `$state` (a store, a class's field),
    `$effect(fn)` and an effect that is an expression, `{@attach}`, or two effects feeding each other. It counts a write in a
    one-line callback as the effect's own, which only says too much.
  - 0.18's one page cache (DC1) takes fetching out of effects, which closes what both leave open.
