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
