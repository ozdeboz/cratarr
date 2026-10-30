# 0063: One health report, every domain's checks; the Health page shows it all, System status what matters most

- Status: accepted (the owner, 2026-10-23: "do we have a proper error framework to automatically surface all of the
  different health indicators for the different domains in our central health page, or do we need deeper
  architecture?"; the Health page first; "needs you shows the most important, the most critical, the most recent")
- Date: 2026-10-23
- Builds on: 0025 (Health: the files the catalogue could not read; the playlists' repairs), 0045 (diagnostics),
  0034 (Incoming's upgrades and duplicates), 0054 (AudioMuse)
- Changes: the Health page (all domains, not the files only), Home's System status (reads this report)

## Context

Each domain said what was wrong in its own way and place: the files on the Health page, the playlists' repairs one
playlist at a time behind its menu, Incoming's upgrades and duplicates on the Library's switches, AudioMuse's coverage
on a tile of its own, a server or Lidarr out of reach on its Settings card, the players, the sign-in and the server's
slow answers and runaway clients as events in the diagnostics log. The Health page showed one domain of eight, Home's
Home's tile (then called Needs you) began as a second collector of its own, and nothing outside could ask Cratarr how it is beyond "up".

## Decision

**One finding.** What a check finds: its `domain` (files, playlists, incoming, analysis, servers, players, sign-in,
server), a stable `key`, a short `title` ("12 files can't be read"), a `count`, a `level` (`critical`: the owner has
to act; `warning`: something is wrong and may right itself; `info`: worth knowing), `since` (when it was first found,
kept across reads while it stays), and `open`: where it is dealt with (a page, with its filter or tab).

**One registry of checks, on the server.** Each domain gives a check that returns its findings now. Cheap ones run as
the report is asked for; costly ones (every playlist against the library) run after what they depend on changed and
keep their answer, saying when they last ran. A check that fails is a finding itself (`warning`, its error), never a
missing report.

**One report.** `GET /api/health/report` (administrators): every finding, worst first, and when each check last ran.
`/api/health` stays the plain liveness answer deploys and proxies use.

**The diagnostics log feeds it, not the other way.** Events stay in the log (0045) as the history; a domain's check may
read its last hour there ("23 slow answers in the last hour"), so what comes and goes shows without another system.

**Its readers.**

- **The Health page** is the first: every finding, by domain, worst first, each opening where it is dealt with; the
  files' sections as now, now one domain among the others.
- **Home's System status** shows what matters most: the most critical first (`critical`, then `warning`), the most
  recent among equals, a few, and a way to the Health page for the rest.
- **The playlists' Repair view** is where the playlists finding opens.
- **Home Assistant** may read it (a sensor per level), as the house's other alerts go to `script.notify`.

**Adding a domain** is adding a check: no page, tile or endpoint of its own.

## Consequences

- System status (lib/HomeStatus; called Needs you until the owner renamed it, 2026-10-23; first built reading the albums and an AudioMuse endpoint of its own) reads the report;
  `/api/audiomuse/progress` becomes the analysis check.
- The Health page's file sections move under the files domain; the files check is the catalogue's counts as before.
- First checks, each small: files (the catalogue's issues), playlists (missing and repeated, all one's playlists),
  incoming (upgrades and duplicates waiting; conversions failed), analysis (AudioMuse's coverage; tracks without
  beats), servers (each connected server and Lidarr reached or not), players (an own player offline for a day; sound
  held back), sign-in (refusals in the last hour), server (slow answers and runaway clients in the last hour).

**Amended 2026-10-23 (the owner):** Home's tile is *System status*, not Needs you; the level the owner must act on is
*critical* (`critical`), not `needs_you`.
