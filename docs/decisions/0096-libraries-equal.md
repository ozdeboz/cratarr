# 0096: Libraries equal; playlists in Cratarr; Incoming a library's setting

- Status: accepted (the owner, 2026-11-02: "this duality is an old one in our software package. My gut feeling says
  more than half of the reasons it exists are no longer real"; by question sets after a5 listed every behaviour the
  split drives). To be built in 0.15/0.16 with each user's own music; the inconsistencies under Context are fixed in
  0.13 as bugs. The server's parts are a5's, the page's 632's.
- Date: 2026-11-02
- Builds on: 0009 (library uses), 0013 (Keep and Replace), 0034 (one Library page, moves), 0044 (folders), 0084 (each
  user's own music); amends 0009, 0034 and 0084

## Context

Every library is Music or Incoming. Of about ten reasons the split drives behaviour, three still held:

- **The playlists' folder.** `.m3u` files live in the one Music folder, their lines relative to it; Incoming's tracks
  are lines climbing out of it (`../lidarr-music/…`, since 2026-10-15). Hence exactly one Music folder, a save refused
  when Music is split, lines rewritten on every move, and no playlists without a Music folder. Nothing but Cratarr
  reads those files now.
- **Lidarr's landing folder.** Lidarr fetches into Incoming; an album leaving it is no longer watched.
- **New music waiting to be heard.** Keep or Delete, Home Assistant's promote or reject, the better-copy and duplicate
  checks.

The rest no longer held or was already alike: crates, New for you, Discover, Radio, the Library's switches, the
Subsonic apps, moves both ways, the trash, permissions, the file writers, analysis and covers treat every library
the same; the Jellyfin reasons are gone with Jellyfin switched off; naming and format on the way in became a
library's own (0034). And the split had left inconsistencies: the checks compared Incoming against Music only (0034
says every library), Keep converted to CD whatever the target said, Health looked only at Music, a playlist's export
dropped Incoming's tracks, and the page still said an Incoming track could not go in a playlist.

## Decision

1. **Playlists are Cratarr's own,** kept in its database by track, as crates are; moves no longer rewrite them.
   `.m3u` files are an export, written for other software. On the first start the existing `.m3u` files are read in
   once; from then on the database is the truth.
2. **Libraries are equal.** Any number of main libraries, each with its own name, colour, naming template and format
   on the way in (one shared template by default). "One Music folder" goes with the playlists.
3. **Incoming is a library's setting** (the word stays), one per fetcher: Lidarr's folder now, Soularr's later (its
   own decision). The fetcher stops watching what leaves it; Undo asks it to watch again.
4. **Only Incoming's records are new.** Keep moves one into a main library: Incoming's default target, changeable at
   each Keep, with that library's naming and format. The better-copy and duplicate checks compare new records against
   every other library.
5. **Health checks every library alike;** a record shows the same problems wherever it is, and Incoming's separate
   "Missing" line goes.
6. **Each user's own music is a main library with an owner** (amends 0084), not a third kind.

## Consequences

- The Music folder's special cases go: the save refusals for Music, the `../` line form, the playlists' store under
  Music, "No Music library is set", the colour dot and the extra delete step tied to "Music" (both follow "main" or
  the library's own colour).
- Album ids that differ by folder (the Subsonic `folder\pid`) stay as they are; a record's data keeps following its
  Music-form id until the model is built.
- The 0.13 fixes: the checks across every library (Replace stays into Music until this is built), Keep honouring its
  target's format, Health over every library, a playlist's export keeping Incoming's tracks, and the page's wording.

## Amendment 2026-11-02: a quality limit in place of the format (for 0.14)

The owner, by question set (relayed by 632): a library's "Albums moved in" (CD format or as they are) becomes a
**quality limit** of four steps: none, up to 24-bit/96 kHz, up to 24-bit/48 kHz, CD (16-bit/44.1 kHz).

- Only lossless files above the limit are converted, into FLAC at the limit; lossy files are never touched.
- It applies to albums moved into the library and to Keep into it; not to Sync to a device or to Export.
- Lowering a library's limit converts nothing already there: Health points out the albums above their library's
  limit, each with its fix.
- Built in 0.14, before the rest of this decision. The server's parts are a5's, the sheet and row 632's.
