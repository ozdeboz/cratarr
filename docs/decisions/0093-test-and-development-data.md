# 0093: Test and development data

- Status: accepted (the owner, 2026-11-01: "Shouldn't we have some sort of test database? Like a development database,
  and also a test database for ephemeral stuff. How much of this makes sense? We want to keep it robust and tight.";
  by question set: all three; reviewed by a5 the same day. Made neutral the same day: how a developer works, and how
  a maintainer tries a release before it ships, are their own and not this project's to direct). For 0.13. The
  server's parts are a5's, the page's 632's.
- Date: 2026-11-01
- Builds on: 0092 (one way to change files: writers that change real files), 0008 (the catalogue), 0011 (the journal)

## Context

- **Tests make their own throwaway data already.** The server's tests open the journal in memory, make about 80
  temporary folders and read a few recorded answers (MusicBrainz, Jellyfin); the page's tests are pure. Nothing is
  shared between runs.
- **There is no library to test against.** No set of real audio files with known tags, and nothing starts the real
  server and drives it end to end. With 0092 the writers change real files (tags, covers, headers, moves), and Undo
  has to put them back exactly: that wants real files, not only unit tests.
- **Nothing to try a change on but a real library.** A page under development pointed at a real Cratarr writes that
  library when a Write is tapped.

## Decision

Two parts every change passes, and one helper anyone may use.

1. **A fixture library, made by the server.** A dozen small albums of generated audio (a few seconds each, FLAC and
   MP3), shaped like the hard cases: two discs, a soundtrack (a compilation of various artists), a classical work with
   movements, a record missing tracks, no year, tags lofty has no key for (STYLE, the MusicBrainz release fields), a
   cover inside the files and one beside them. `cratarr-server --make-fixtures <folder>` makes it, the same tags every
   time (building on `retag::fixtures` and its tiny generated covers); no audio or image file is committed (the
   repository is public, and binary churn is noise).
2. **One end-to-end suite, ephemeral.** It copies the fixture library and a fresh data folder into a temporary place,
   starts the real server binary on them, and drives its API: the catalogue read, 0092's plan → apply → Undo, "changed
   since it was planned" (a file changed between plan and write), the lock ("waiting"), the journal kept across a
   restart (the same binary stopped and started on the same data folder). Connections are off and the time is fixed
   where it matters. Everything is thrown away after the run. A stage of its own in the Dockerfile, after `test`.
3. **A development instance, optional.** `scripts/dev-instance.sh` starts a second Cratarr in docker with a data
   folder of its own and the fixture library, plus any albums of the developer's own in its music folder
   (`scripts/dev-sample.sh` copies a list of them read-only from a music server over SSH; the list stays out of the
   repository). Its writers change only its copies.
   **Fenced off from the world from its start:** MusicBrainz switched off at its first start (its one request a second
   is shared with any other Cratarr on the same address); no accounts, so no LDAP and no service sign-ins; never
   connected to Lidarr, slskd or Jellyfin; 768 MB, as the compose file's.

**Not done:** copies of Lidarr, slskd or Jellyfin. Their APIs are faked in the tests from recorded answers, as
MusicBrainz's are.

## Consequences

- A writer is proved on real files, Undo included, before it reaches a library.
- A page under development can be tried without touching a real library.
- How a developer works is their own: what a pull request must pass is the tests, the end-to-end suite among them
  ([CONTRIBUTING.md](../../CONTRIBUTING.md)). The development instance is a convenience, never a requirement.
- A preview is built only once the end-to-end suite passes, and a writer reaches one only after it was proved there.
  Where a maintainer runs a preview before release is theirs to decide and is not recorded here.
- A development instance's data is never deployed or copied to a production server.
