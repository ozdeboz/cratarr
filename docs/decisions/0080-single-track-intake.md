# 0080: Single-track intake: one track into an album, checked before it goes in

- Status: accepted (the owner, 2026-10-26, for 0.9.8: single-track replacement as planned; the AcoustID gate at 0.9)
- Date: 2026-10-26, written down 2026-10-30
- Builds on: 0010 (one writable mount), 0013 (Incoming: hi-res made CD format), 0053 (covers inside the files)
- Amended by: the amendment of 2026-10-30 below (a file already on the server as a second source)

## Context

An album missing a track, or holding a damaged one, could only be mended by fetching the whole album again, and often
the whole album cannot be had. In March 2026 unattended Soulseek batches put wrong files into the library; whatever
takes a single track in must not repeat that.

This record writes down what 0.9.8 built (`server/src/app/track_fetch.rs`, `slskd.rs`, `acoustid.rs`), so that the
change below has a decision to amend.

## Decision

One track at a time, never bulk, the administrator choosing each step:

1. **The release.** The album's MusicBrainz release (the one Incomplete found, else its files' own, else its match's),
   each track with the file that holds it, or none.
2. **The source.** One slskd search for one of those tracks, from MusicBrainz's artist, title and length; FLAC and MP3
   only. The administrator picks a file.
3. **The fetch and the gate.** The file goes into its own folder under slskd's (`cratarr/<batch>`), is moved out to
   Cratarr's staging at once, and is checked: decoded whole without a damaged frame, within two seconds of the
   recording's length, and named that recording by AcoustID at a score of 0.9 or more. A file that fails is refused,
   with why.
4. **Add to album.** A passed file is tagged from MusicBrainz (title, artist, its place, its ids) and the album's own
   tags (from its nearest neighbour), named as its neighbours are, its cover put inside, and copied into the album's
   folder; hi-res is made CD format first, as Incoming's is. A track it replaces goes to the trash. Undo takes the new
   track out and puts the replaced one back.

## Consequences

- Nothing reaches the library unchecked: the gate stands between every source and the album.
- A track's place on the album is the release's, never what the file says about itself.
- Searches and fetches live in memory; a restart forgets them, and their staged files are tidied a day later.

## Amendment (2026-10-30): a file already on the server

Missing tracks turned up on old drives. The owner: "Cratarr's intake, in 0.9.19": they are taken in through this
intake, not put into the album folders by hand.

- **A second source.** Beside the Soulseek search, a file already on the server. The allowed place is Cratarr's own
  folder under slskd's completed downloads (`<slskd folder>/cratarr`, `/slskd/cratarr` in the container), and any
  folder the owner makes in it. Nothing else: a path is named relative to that folder, with no `..` and nothing
  absolute, and it is resolved (links followed) and must still be inside it. The per-fetch folders slskd downloads
  into (named by their batch's id) are left out.
- **The same gate, the same add, the same Undo.** The file is copied into staging, never moved: the owner's original
  stays where it is. From there it is a fetch like any other: checked, then Add to album or dropped.
- **The recording decides the place.** The track's number and disc come from the release and the recording chosen,
  never from the file's own tags (one of the files found is numbered 06 in its tags and belongs at 07).
- **Still one at a time.** One file per request; the list of files is for picking one.
- **The routes.** `GET /api/track-files` (with `?q=`, the best match for its words first) lists the FLAC and MP3 files
  there with their size, length and tags; `POST /api/track-files/fetch` takes one, with the album, recording and
  track to replace as a search names them, and answers with a fetch's id. A fetch of its own rather than a fetch of a
  search: a search is a Soulseek search, which a file on the server does not need (nor slskd running or switched on,
  only its folder set).
