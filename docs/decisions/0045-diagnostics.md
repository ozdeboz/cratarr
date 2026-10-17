# 0045: Diagnostics: what happened, kept by Cratarr itself

- Status: proposed
- Date: 2026-10-17
- Builds on: 0031 (Play On), 0033 (Cratarr holds the queue), 0037 (sync play), 0043 (sign-in everywhere)

## Context

On 2026-10-17 the owner's phone played a queue Cratarr had handed over to the salon tablet 8½ minutes before. The
cause was found (a handover cannot stop a player that is offline, and the queue given back when it returns was that
stale one), but only by reading Cratarr's, Caddy's and the phone's logs side by side, and one part stays a guess: why
the phone's Play On connection stayed down for ten minutes while its sync play came back within seconds. The likely
answer (the app waits ten minutes after a 401 or 403) left no trace anywhere that could be read:

- The phone's log (logcat) is 256 KiB per buffer, about 15 minutes there; the app's own lines (`CratarrPlayOn` and
  the others) did not reach it at all.
- Cratarr logs that a player connected or disconnected, not why; a refused request, and a command that could not be
  delivered (the handover's stop), are not logged.
- The container's log is lost each time a new build is deployed (the container is made again), so Cratarr's side
  keeps a few hours at most.
- Lines of the three are matched by their times only.

Cratarr runs in homes, on small machines (the owner's: 8 GB, about 3 GB free), one household each, and is public: what
it needs to explain itself has to come with it, and nothing may leave the install.

## Decision

1. **Cratarr keeps a diagnostics log of its own**, in its data folder (`diagnostics.sqlite`), apart from the undo
   journal (0011, 0026). One row per event: when, where it happened (the server, or a player by name), the area (Play On,
   sync play, sign-in, library, Subsonic, imports), what, and its details. Recorded:
   - a player connecting and disconnecting, and why it ended (closed by the player, a write that failed, replaced by
     a newer connection, gone quiet);
   - a request refused (401, 403), with the address and the client it came from;
   - a command that could not be delivered, and to whom;
   - handovers, joins and leaves, queues given back, and queues adopted from a player;
   - library rebuilds and why, imports, restarts with their version.

   Kept 14 days, at most 100,000 rows, the oldest going first. No secrets in it: keys, passwords and sessions are left
   out, not masked.
2. **The apps keep their own small log and hand it to Cratarr.** The Android app writes its link and service events to a
   file on the device (the last 2,000 lines): each connection attempt with the address used and how it went, waits and
   why (the ten minutes after a refusal among them), switches between the home and public address, the player
   service starting and stopping, and crashes. On every connect it sends what Cratarr has not had yet; Cratarr adds it
   to the log under that player, its times put on Cratarr's clock (the offset sync play keeps). The page's own player
   does the same with what it keeps while open. Nothing goes to anyone else.
3. **A player's link is part of what it reports**: connected since, attempts that failed since then, the last error,
   the address in use, and a wait it is in (until when, and why). `GET /api/players` shows it, so Home Assistant and the
   page see a player in trouble while it is.
4. **One id per connection**: the app makes one for each Play On connection and sends it with every request
   (`X-Cratarr-Connection`); Cratarr writes it with that player's events and sends a request id back on every answer
   (`X-Request-Id`). A proxy that logs these headers lines its log up with Cratarr's; setting that up is the install's
   (for the owner's: nas-infra), not Cratarr's.
5. **Settings → Diagnostics**: the log, newest first, by player and area; each player's link as it is now; how much is
   logged by area (more for an hour or a day, then back by itself, without a restart); and **Download**, one file of the
   log, the players and the settings without secrets, for a bug report.
6. **Not now:** Prometheus metrics (`/metrics`, off by default) wait until someone asks; a monitoring stack (OpenTelemetry,
   Grafana, Loki) and outside crash or telemetry services are not part of Cratarr. Whether Cratarr is up stays with the
   install's own monitoring (for the owner's: the NAS monitor and Home Assistant).

The page shows the log as it is, with no prose; errors are one short line (ui-conventions).

## Consequences

- The next "why did that happen" is read on one page, across server, phone and tablet, days later and after a deploy.
- A player stuck waiting or failing to connect shows in `/api/players` and Home Assistant while it lasts; an alert on
  it is the install's choice (ha-infra).
- A bug report from someone else's install can carry what happened, without them reading logs.
- The log stays small: by estimate a few thousand rows a day, so a few MB at the 14 days' end; the phone's file is
  bounded by its 2,000 lines.
- The app's lines missing from logcat get found and fixed on the way (item 2's file must not depend on logcat).

## Who builds what

- The log itself: its file, rows, retention, queries and the download's contents (a new `server/src/diagnostics.rs`),
  and the events from the library, Subsonic and imports (`serve/`, `userdata/`, `source/`): claude-a5.
- The events from Play On and sync play (`app/playon.rs`, `snapcast/`), the refusals and the request ids (`api.rs`),
  the routes, area log levels at runtime (`main.rs`), the Android app's file, its upload and the link in its reports
  (`android/`), the page player's part and the Diagnostics page: claude-5f.
