# 0099: Soulseek in Cratarr — whole records fetched by their release, on a tick

- Status: accepted (the owner, 2026-11-02: "Cratarr does Soulseek, Lidarr stays", in 0.15; the choices below by question
  set in a5's session, 2026-11-03)
- Date: 2026-11-03
- Builds on: 0080 (single-track intake and its gate; its amendment, the bulk intake), 0034 (Incoming), 0096 (a library's
  fetcher is a setting), 0092 (one way to change files: the intake card), docs/plans.md (Soulseek in Cratarr, not through
  Lidarr; the lessons from SoulSync and DroppedNeedle)
- Replaces: soularr

## Context

soularr picked a whole folder from one Soulseek user by its name and handed it to Lidarr, which imported what landed,
unchecked. In its 72 hours to 2026-10-31: about 40 albums imported, 34 failed outright after retries, 6,869 enqueues
and 945 folder looks refused, 109 downloads broken off. Cratarr already searched and gated single tracks well (0080).

## Decision

**Cratarr fetches whole records from Soulseek; Lidarr keeps watching artists and grabbing torrents.**

- **What is wanted:** Lidarr's Wanted (monitored and missing), read every six hours; Cratarr's approved Requests among
  them (they go to Lidarr) are searched first.
- **Judged by its release:** Lidarr's monitored release, else the release group's first official one with tracks, as
  MusicBrainz lists it. A Soulseek folder is a candidate only when every track of the release is one of its FLAC or
  MP3 files, by title and length (within 2 s), never because its name looks right. Best first: lossless, a free slot,
  a short queue, the fewest extra files, the fastest.
- **Searched on a ladder**, none twice: the record and its year; its title without what is in brackets; the main artist
  only; a wildcard for the artist's first letter (Soulseek's server leaves some names out of every search). One search
  at a time, paced. Not found, searched again later: after 15 minutes following none or one miss, four hours after two,
  a day after three, a week after four or more; never given up; Search now overrides.
- **Fetched only with a tick:** the administrator approves many at once. The folder's tracks come in one batch.
- **A user who fails** (refused, offline, an error, or queued over 30 minutes) is remembered for the record, and the
  next best folder is fetched by the same tick (the owner, 2026-11-03). A read of slskd that fails once is not the end.
- **Every file gated** as single-track intake gates it: decoded whole, the recording's length, its sound, AcoustID. A
  folder with a file that fails is set aside whole, with why per file, and the next folder tried (the owner,
  2026-11-03). Only a whole, checked record goes on.
- **Into its own Incoming library** (the owner, 2026-11-03): the Incoming library whose policy names the fetcher
  "soulseek" (0096), as Artist/Record (Year), copied in under a hidden name then renamed; never into Music, never into
  Lidarr's folder. From there the intake card (0092) takes it.
- **Kept on disk** (`soulseek.json` in the data folder): a fetch under way is followed again after a restart. Nothing
  is searched until an Incoming library takes Soulseek's records.

The bulk intake of missing tracks takes the same lessons: kept on disk, the ladder's waits, failed users remembered,
the next candidate by the same tick, a file that fails the gate set aside with why.

## Consequences

- soularr is stopped when this is deployed, and the NAS gives Cratarr a writable folder for the new library (a NAS
  change, on the owner's go).
- The page lists Lidarr's Wanted with each record's state, folders and set-asides, ticks many at once, and offers
  Search now (632).
- Not planned: other sources (Usenet, streaming). With each user's own music (0084, 0.16): the intake per library.

## Amendment (2026-11-03): the fetchers are siblings

The owner, 2026-11-03: "If people have Lidarr in their stack and want to keep using it, then that's legitimate. And
if people don't want Lidarr in their stack and want to use slskd directly, this should also be possible. And from our
app, it shouldn't make a whole lot of difference. Sure, each fetcher has its configuration, and slskd requires more
configuration that Lidarr abstracts for our app. But they are still siblings from our point of view."

- **Two fetchers, either or both:** Lidarr (it watches artists and grabs, through its own downloaders) and Soulseek
  (Cratarr through slskd, as above). Each works without the other.
- **What Soulseek fetches:** Cratarr's approved Requests, and nothing of Lidarr's (the owner, 2026-11-03: "The whole
  idea is that Cratarr's Soulseek doesn't need Lidarr"): it never reads Lidarr's Wanted. Lidarr fetches its own Wanted
  through its own downloaders. Without Lidarr nothing watches artists for new releases: that is what Lidarr adds.
- **A request approved** goes to every fetcher connected: to Lidarr as now, and into Soulseek's list.
- **One shape in the page:** Requests lists one list per fetcher connected, each the same: what it fetches now, what
  it has not found yet (Lidarr, Soulseek; "Monitor" was Lidarr's). Settings › Connections groups them as Fetchers,
  each its own page (Lidarr: its address and key; Soulseek: slskd's address, key and folder). An Incoming library's
  "Fetched by" names its fetcher (0096: one Incoming library per fetcher).
- This replaces the decision's "What is wanted" above (Lidarr's Wanted, read every six hours) for Soulseek.
- The server's part (Soulseek's wanted list from Cratarr's Requests, a library's fetcher "lidarr" beside "soulseek")
  is a5's; the page's 632's.


## Amendment (2026-11-06): what could not be asked is no verdict

An outside service gone away says nothing of a file or a user (the audit's finding 11). Decision 0099 lets the next
folder go when a file fails; an outage is not a failed file.

- **AcoustID could not be asked** (away, over its limit, its key gone or never set): the record is held at its check,
  as it is: its tick, its folders, what it fetched, its user. Its row says why, and it is checked again every quarter of
  an hour. A file that failed before it stands: the folder is set aside.
- **One held can leave the list**, and what it fetched with it.
- **The bulk intake** has no folder to wait in: the track stops, says why, and Search now takes it up again. No user is
  remembered and no next candidate fetched.
- **slskd could not be asked** (no answer, its key refused, not up, or not signed in to Soulseek): no user failed. An
  approved record stays approved with its folders, a fetch under way keeps its batch and is followed again, and a search
  counts no miss. Tried again in five minutes. Only slskd answering that it no longer knows a batch ends that fetch. The
  bulk intake's track stops and says why, as above.
