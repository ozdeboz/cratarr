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

1. **Every GET the page sends passes one guard** (`lib/loopGuard`). The same address asked more than 20 times in 10 s
   fails at once for the rest of that window, said once in the console with its address. Polls, at most one every 2 s,
   and every write are never held. A burst that settles is let through again.
2. **A page test scans every effect** for one that reads and writes the same `$state`. A new one fails the tests unless
   it is reviewed and listed as harmless: it settles in one pass, or its write is an unchanged value.
3. **A failure is never "nothing".** A server error answers as an error, not as "no cover", and the page asks again
   later. Only a real "none" is remembered.

## Consequences

- A loop shows up as a held request and a console line, not as a slow server.
- Writing an effect that both reads and writes state needs a reason, written in the allowlist.
