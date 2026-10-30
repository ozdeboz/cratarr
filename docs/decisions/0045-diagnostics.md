# 0045: Diagnostics: what happened, kept by Cratarr itself

- Status: accepted (the owner, 2026-10-17: "Proceed.")
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
   - a player connecting and disconnecting, and how its connection ended: replaced by a newer one, gone quiet (its
     lease ran out), closed (by the player or the network; the server cannot tell which, the player's own log can),
     or the server stopping;
   - a request Cratarr refuses (401, 403) in its sign-in layer, and what it answers a proxy asking it to check a
     sign-in, with the client and the address asked for. A refusal made in front of Cratarr (a proxy's own rules, a
     bouncer) never reaches it and is not in this log;
   - a command that could not be delivered, and to whom;
   - handovers, joins and leaves, queues given back, and queues adopted from a player;
   - library rebuilds and why, imports, restarts with their version.

   The same event again within a minute (the same client, address and answer for a refusal; the same warning) adds to
   a count on its row instead of a new row, so one prober or one stuck device cannot fill the log. Kept 14 days, at
   most 100,000 rows, the oldest going first. No secrets in it: keys, passwords and sessions are left out, not
   masked.

   The log is fed by the server's own logging (`tracing`): the places that log events with their area and fields, and
   the log takes them through one layer that writes on a thread of its own, never on the caller's. What the log keeps,
   by area, is the same setting as what the server prints (item 5).
2. **The apps keep their own small log and hand it to Cratarr.** The Android app writes its link and service events to a
   file on the device (the last 2,000 lines, numbered): each connection attempt with the address used and how it went,
   waits and why (the ten minutes after a refusal among them), switches between the home and public address, the
   player service starting and stopping, which side closed a connection, and crashes. On every connect it sends the
   lines Cratarr has not had yet; Cratarr answers with the last number it holds, so sending again adds nothing twice.
   Cratarr adds them to the log under that player, their times put on Cratarr's clock (the offset sync play keeps; lines
   from before the first measurement get the offset known at sending). The page's own player does the same with what it
   keeps while open. Nothing goes to anyone else.
3. **A player's link as Cratarr knows it** is part of `GET /api/players`: online, offline since, last heard from, and how
   its last connection ended. While its reports still get through (its command stream down, say) it adds its own
   view: connected since, attempts that failed since, the last error, the address in use, and a wait it is in (until
   when, and why). A player that cannot reach Cratarr at all cannot say more; its side arrives with its log once it is
   back (item 2). So Home Assistant and the page see a player go missing while it is, and why, afterwards.
4. **Ids that tie the logs together**: the app makes an id for each Play On connection and sends it with every request
   of its own (`X-Cratarr-Connection`); the page's player sends its own id with its reports (a page's requests cannot
   carry a header the app sets). Cratarr writes the id with that player's events, and answers every request with a
   request id (`X-Request-Id`): the one a proxy sent, else one of its own. A proxy that logs these headers lines its log
   up with Cratarr's; setting that up is the install's (for the owner's: nas-infra), not Cratarr's. Sync play's sound
   is not HTTP and carries none; its events have the player's name.
5. **Settings → Diagnostics**: the log, newest first, by player and area; each player's link as Cratarr knows it; how
   much is logged by area (more for an hour or a day, then back by itself, without a restart; the same setting governs
   the log and what the server prints); and **Download**, one file of the log, the players and the settings without
   secrets, with client addresses masked, for a bug report.
6. **Not now:** Prometheus metrics (`/metrics`, off by default) wait until someone asks; a monitoring stack (OpenTelemetry,
   Grafana, Loki) and outside crash or telemetry services are not part of Cratarr. Whether Cratarr is up stays with the
   install's own monitoring (for the owner's: the NAS monitor and Home Assistant).

The page shows the log as it is, with no prose; errors are one short line (ui-conventions).

## Consequences

- The next "why did that happen" is read on one page, across server, phone and tablet, days later and after a deploy.
- A player that goes missing shows in `/api/players` and Home Assistant while it is gone; why shows once it is back.
  An alert on it is the install's choice (ha-infra).
- A bug report from someone else's install can carry what happened, without them reading logs.
- The log stays small: by estimate a few thousand rows a day, so a few MB at the 14 days' end; the phone's file is
  bounded by its 2,000 lines.
- Adding an event is one logging line where it happens; no part of Cratarr calls the log directly.
- The app's lines missing from logcat get found and fixed on the way (item 2's file must not depend on logcat).

## The work

- The log itself: the layer, its writer thread, the file, rows and counts, retention, queries and the download's
  contents (a new `server/src/diagnostics.rs`), its place in the server's logging setup with the area levels that
  change at runtime (`main.rs`), and the events from the library, Subsonic, imports and sign-in, the refusals among
  them (`serve/`: the sign-in requirement, the proxy's check, the sign-in page, Subsonic logins).
- The events from Play On and sync play (`app/playon.rs`, `snapcast/`), the request ids (`api.rs`),
  the routes, the Android app's file, its upload and its link in its reports (`android/`), the page player's part, and
  the Diagnostics page.

## Amended (2026-10-23): runaway clients, slow answers, whose address

Cratarr had no request-rate warning of its own, so a runaway client went unflagged (the owner: "This is a problem we
can own"). The Settings page was one: its effect read the editor's state while it loaded, and each answer changed that
state, so it asked again without end, and each `GET /api/settings` asked Jellyfin and Lidarr in turn. After reading
Jellyfin's own (a slow-response line, at Debug only; KnownProxies; no rate limit at all), three additions:

- **A runaway client** (`serve::rate`): every request counted per client by the minute. A client asking more than 600
  times a minute, two minutes running, is a warning (area `server`, "client requests too often"): its address, how
  often this minute and the last, what it asked most (the path's first three parts) and how often, its last connection
  id. Two minutes, so a page loading a shelf's covers at once is not one. Nothing is refused: the proxy throttles.
- **Slow answers** (`api::request_ids`): every answer says how long it took to start (`X-Response-Time-Ms`); over 1 s it
  is a warning ("slow response": client, path's first three parts, status, ms). Not Debug, as Jellyfin's: unseen is
  no warning.
- **Whose address** (`serve::proxy`, as Jellyfin's KnownProxies): with `CRATARR_PROXIES` set (addresses or ranges), the
  `X-Forwarded-*` headers of any other peer are taken off before anything reads them, so a client reaching the port
  itself cannot name another address to slip the sign-in throttle or the count above; the first such request of each
  peer is a warning. Unset, every peer is believed as before, and the first that forwards is written down (info) so it
  can be named.
- **The Settings page** loads once per administrator (`untrack`), and the servers' answers for it are kept 10 s for the
  same settings and folders (`SettingsService::asked`), so a page that asks again does not each time reach Jellyfin.

Not taken from Jellyfin: locking an account after failed sign-ins until an administrator opens it (anyone knowing a
name could lock its owner out; Cratarr's throttle answers 429 with a wait), and uploaded client logs (the players'
numbered logs already come in).
