# 0090: Cratarr as an MCP server

- Status: accepted (the owner, 2026-10-31, via the ThinkPad session: "Yes, 0.13 MCP server built inside"); for 0.13
- Date: 2026-10-31
- Builds on: 0043 and 0051 (sign-in, accounts, API keys and their scopes), 0081 and 0089 (release types and why; facts
  set by hand), 0082 and 0086 (classical, works)

## Context

The owner reviews the library with an assistant (Claude Code on the ThinkPad): which records are EPs or singles, which
are classical and why. The assistant had no way to read Cratarr's answers: the API needs a sign-in or a key, and the
reasons were not kept anywhere. Existing MCP servers for Subsonic, Navidrome and Jellyfin were looked at first; they
reach a music server through its API from outside, and none knows Cratarr's types, reasons, works or crates.

## Decision

- **Inside the server**, not the desktop app (its MCP stays shelved, plans.md): `/mcp`, the MCP Rust SDK's (`rmcp`,
  Apache-2.0) streamable HTTP, a thin layer over the server's own reads.
- **A key only.** Behind the same access check as the API, and only for an API key (Settings → Authentication); no
  sign-in cookie, no open or loopback way in. A reading key is enough: every tool reads, none changes anything, so a
  reading key may POST there (the protocol's way) and nowhere else. Writing tools come only if the owner asks.
- **Few tools, each with filters or an action** (as Jellyfin's grouped MCP tools, to keep a client's list short):
  `search`, `albums` (records by type, classical, set by hand, artist; each with its type and where it came from, the
  rule that decided it, what is set by hand, and whether it is classical and why), `album` (one record and its
  tracks), `classical` (composers, a composer's records or works, a work), `health` (the library's findings, missing
  tracks among them), `crates` (the account's crates, or one crate's tracks, as the key's account).
- **Any Host**: the SDK's guard against DNS rebinding (loopback names only) is off, since a key is needed, which a page
  another site opens cannot send.

## Consequences

- An assistant reads what the page shows, with the reasons, from the key the owner makes and can revoke.
- The page and the MCP server share one set of reads; a tool is a few lines over them.
