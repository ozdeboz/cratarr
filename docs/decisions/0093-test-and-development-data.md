# 0093: Test and development data

- Status: proposed (the owner, 2026-11-01: "Shouldn't we have some sort of test database? Like a development database,
  and also a test database for ephemeral stuff. How much of this makes sense? We want to keep it robust and tight.";
  by question set: all three, the development instance on this PC, fixtures and a real sample). For 0.13. The server's
  parts are a5's, the page's 632's.
- Date: 2026-11-01
- Builds on: 0092 (one way to change files: writers that change real files), 0008 (the catalogue), 0011 (the journal)

## Context

- **Tests make their own throwaway data already.** The server's tests open the journal in memory, make about 80
  temporary folders and read a few recorded answers (MusicBrainz, Jellyfin); the page's tests are pure. Nothing is
  shared between runs.
- **There is no library to test against.** No set of real audio files with known tags, and nothing starts the real
  server and drives it end to end. With 0092 the writers change real files (tags, covers, headers, moves), and Undo
  has to put them back exactly: that wants real files, not only unit tests.
- **There is no development instance.** The LAN dev page on this PC talks to the NAS's real Cratarr, and the previews
  are deployed onto it. A Write tapped while trying the dev page would write the real library.

## Decision

Three tiers, each with data of its own; production is changed only by a deploy the owner gives the go for.

1. **A fixture library, in the repository.** A dozen small albums of generated audio (a few seconds each, FLAC and
   MP3), shaped like the hard cases: two discs, a soundtrack (a compilation of various artists), a classical work with
   movements, a record missing tracks, no year, tags lofty has no key for (STYLE, the MusicBrainz release fields), a
   cover inside the files and one beside them. Made by a script, so it can be made again; small enough to keep in git.
2. **One end-to-end suite, ephemeral.** It copies the fixture library and a fresh data folder into a temporary place,
   starts the real server binary on them, and drives its API: the catalogue read, 0092's plan → apply → Undo, "changed
   since it was planned" (a file changed between plan and write), the lock ("waiting"), the journal kept across a
   restart. The page can be driven against the same server in headless Edge. Everything is thrown away after the run.
   It runs in the existing test container, and only one heavy job runs at a time (this PC's rule).
3. **A development instance, on this PC.** A second Cratarr in docker with its own data folder (its own catalogue,
   user data and journal) and its own music: the fixture library plus a sample of about 100 real albums, copied read
   only from the NAS (the NAS is read, never written). The LAN dev page and preview builds point at it; its writers
   change only its copies. It is kept until its data is thrown away by hand, and made again from the fixtures and the
   sample by one script. It is up while the PC is.

**Not done:** copies of Lidarr, slskd or Jellyfin. Their APIs are faked in the tests from recorded answers, as
MusicBrainz's are.

## Consequences

- A writer is proved on real files, Undo included, before it reaches the library.
- The dev page can no longer change the real library; trying a change on the owner's own music needs a deploy and its
  go, as it always should have.
- The sample of real albums stays on this PC, inside the development instance's data; it is not in the repository.
- Previews move from the NAS's production instance to the development instance; what the owner sees on the phone from
  the production address changes only with a release.
