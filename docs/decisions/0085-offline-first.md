# 0085: Offline first, in five steps

- Status: accepted as a plan (the owner, 2026-10-30: "This sounds like a pretty good plan"; its steps settled the same day:
  "Yes. Write this in."), beside 0.11 to 0.14; not built
- Date: 2026-10-30
- Builds on: 0033 (Play On's queues), 0038 (kept on this device), 0052 (local library), 0057 and 0075 (one engine, the
  rules crate), 0062 (the app owns its controls), and the page's out-of-reach rules (lib/reach, lib/later)

## Context

Cratarr's apps are thin clients, as Jellyfin's are: the Android app and the desktop app show the server's pages, and
the pages ask the server for everything. What the owner likes in Finamp is its offline-first reliability, not its
native look. Today the apps already keep music on the device (Sync to this device, Download…), show the last copy of
what they saw when the server is out of reach (the service worker), and send the changes made meanwhile once it answers
again (lib/later). But a cold start with no network depends on what was last cached, nothing but the downloads can be
searched or filtered offline, and an edit made offline is replayed rather than merged.

Offline first is a question of where the data lives, not of the UI's toolkit. So the Svelte pages stay, and the
apps gain what a thick client has: their own pages, a local store read first, and a sync with the server.

## Decision

Five steps, each useful on its own, so offline use improves with every release rather than at the end:

1. **The apps carry their own pages** (beside 0.11). The APK and the `.deb` hold the web app; the server sends data
   only. The app opens at once anywhere, a blank page never. An app whose pages are older than the server's takes the
   server's (the server says its pages' version; the app compares).
2. **Downloads, an offline shelf** (beside 0.12). What is on the device keeps its metadata and covers in a local store,
   so Downloads searches, filters and orders with no network.
3. **Nothing done offline is lost** (beside 0.12). Favourites, playlist edits, plays and scrobbles are written to the
   device first and synced, with each change's merge rule set (open points below); a small "N to send" shows what
   waits. What lib/later does today, made the rule.
4. **The whole library's index on the device** (beside 0.13). Every album's, track's and artist's metadata (a few MB,
   no audio), kept current by what changed since the last sync; what is not downloaded shows dimmed. Offline the whole
   library browses, searches and filters; online the pages read the device first, so they are instant. Modes per
   server (0.13) decide what each mode mirrors, so this comes after them.
5. **Crates and the queue on the device** (0.14). Crate rules run over the local index through the rules crate (0075's
   one engine, the shared Rust core: in the app through UniFFI, in the desktop app natively), and the queue is held on
   the device, synced with Play On. Crates, radio and handovers work without the server in between.

The local store and its sync are one piece of the shared Rust core (SQLite and the sync), built once for the Android
app and the desktop app; the generated protocol types (lib/generated) are its messages.

*Amended 2026-10-30 (the owner, "Page store now, Rust in 0.13"):* step 2's store is the page's own IndexedDB in 0.12
(lib/shelfStore: the downloaded tracks' details and small covers, in the app's WebView, the desktop app and an installed
page alike); it moves into the Rust core's store once, with step 4 in 0.13, rather than being built twice.

## Graceful degradation, first

The owner (2026-10-30): "graceful degradation is the way to go in first instance, so we don't show what we cannot
consume, and we don't offer what we cannot uphold." Wherever the apps meet a server that is older or newer, or none
at all, they show only what they can read, and offer only what they can carry through: a field the pages do not know
is left out, not half-shown; an action the server (or being offline) cannot complete is not offered, rather than
offered and refused. What the steps below settle, they settle in that light, with what Symfonium and Finamp do as reference.

## Settled, step by step (the owner, 2026-10-30)

Studied against Finamp (it blocks what it cannot merge, sends plays never, keeps only downloads, and goes by what each
item says it allows) and Symfonium (offline-first playlists replaced whole one way or the other, plays sent later but
collapsed per song, the whole library's index on the device, a version allowlist that turned a newer server away).
Both are held back by servers without a change log, ids per entry or times; Cratarr's server is its own, so it gives
them. Each step brings only as much of the mechanism as it needs, and what it cannot yet uphold is not offered.

- **Beside 0.11: the app opens anywhere and never offers what it cannot do.** The apps carry their own pages (step 1).
  The server says what it can do (a list of capabilities, not a version number: /api/health), the app keeps the last
  answer for offline, and the pages leave out what it lacks; items carry their own allowances where they differ
  (can edit, can delete). A degradation says so in one line where it happens, never as a refusal after a tap.
- **Beside 0.12: Downloads wholly offline, and no play lost.** Downloads is the offline shelf (step 2): its metadata and
  small covers on the device, searched, filtered and ordered offline. Plays and scrobbles made offline are kept with an
  id and a time each, sent once they can be, never twice and never collapsed. Favourites and ratings work offline, the
  last change by time winning. The server keeps a change log, and a playlist's edits become operations with ids (add a
  track, remove entry 7, move entry 7 after 3), replayed on the server and merged: additions and removals as a set,
  the order as the last move made it; so one's own playlists can be edited offline (step 3 whole).
- **Beside 0.13: the whole library browsable.** After modes per server, which decide what each mode mirrors, the whole
  library's index syncs by the change log (step 4): "Available offline" shows what plays, the rest is dimmed; small
  covers for the index, full ones for what is downloaded.
- *Amended 2026-10-30 (the owner):* modes per server moved from 0.12 to 0.13, and the change log with playlist edits
  as operations came forward from 0.13 into 0.12.
- **0.14: crates and the queue on the device.** Crate rules run on the device through the rules crate (the shared Rust
  core, step 5). A queue played offline stays that device's own; Play On takes it up on the way back. A crate's rules
  are edited online only, until the change log covers crates too.

## Consequences

- Each step is a release's piece of its own and can stop there: after step 3 the apps open anywhere, Downloads is
  fully offline, and nothing done offline is lost.
- The desktop app's move from Electron to a Rust shell (Tauri) is natural from step 5, not required before it.
- The server gains a sync: what changed since a version, per kind, and the merge of what comes back.
