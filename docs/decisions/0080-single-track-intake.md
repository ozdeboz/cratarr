# 0080: Single-track intake: one track into an album, checked before it goes in

- Status: accepted (the owner, 2026-10-26, for 0.9.8: single-track replacement as planned; the AcoustID gate at 0.9)
- Date: 2026-10-26, written down 2026-10-30
- Builds on: 0010 (one writable mount), 0013 (Incoming: hi-res made CD format), 0053 (covers inside the files)
- Amended by: the amendments of 2026-10-30 below (a file already on the server as a second source; the gate hears
  the sound: fake lossless, re-encoded MP3s, clicks, pops and clipping)

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

## Amendment (2026-10-30, later): the gate hears the sound

The owner, for 0.10: the gate gains two checks, for Soulseek's finds and files on the server alike. A file failing one
is refused with why, as a wrong length is, and the page shows the why as it is. Both run on the decode the gate makes
already (`acoustid::print`, the whole file), in one pass over its sound mixed to mono (`analysis::flaws`); nothing is
decoded twice. They come after the damaged-frame and length checks and before AcoustID is asked.

- **Where its highs stop: fake lossless, and MP3s made from lower ones.** A Hann-windowed FFT of every 2,048 samples,
  the power averaged over the track, then the highest brick wall in it: 30 dB or more lower in the 750 Hz just above an
  edge than in the 750 Hz just below it (their centres 1.25 kHz apart), and everything above it at least 20 dB below
  the band below. The edge is placed to a bin (21.5 Hz at 44.1 kHz) and told to the nearest half kHz.
  - A file sold as lossless (FLAC, ALAC, WAV and other PCM, WavPack, APE, TTA) with one is refused: "Not lossless: it
    stops at 16 kHz".
  - An MP3 whose highs stop 2 kHz or more below the low pass LAME gives its bitrate (its audio's bytes over its length:
    up to 128 kbit/s 16 kHz, to 160 17.5, to 192 19, above 20) is refused: "Re-encoded from a lower bitrate: it stops
    at 16 kHz". A 320 with a 128's or a 160's cutoff is refused; one with a 192's passes (near enough). Other lossy
    formats are not judged by their highs.
  - Only an edge from 14 kHz to 20.2 kHz, and 1.5 kHz or more below the file's Nyquist, counts: a converter's
    anti-alias filter near 21 kHz is not a lossy encoder's. A natural roll-off is never a refusal, however low or quiet
    (old recordings, solo piano, ambient): it never drops 30 dB in 1.25 kHz. Better to pass a real file than refuse it.
- **Clicks, pops and clipping, in sound that decodes fine**, by the nightly library scanner's rule, so the two agree.
  The scanner (Essentia's ClickDetector, frames of 512 every 256, on the NAS) was calibrated on the library: a musical
  transient sits 2 to 33 dB above loud context, while Cri D'Amour's ending (its last ~32 ms jumping to full scale after
  ~2.8 s near -84 to -93 dB) sits 92 to 94 dB above it; raw click counts mean nothing (electronic music has dozens of
  harmless ones). Here:
  - a click is a sudden jump from one sample to the next (the second difference) in a hop of 256, 20 dB or more above
    the jumps before it; it is a defect when the peak at it (a hop either side) is more than 60 dB above the RMS of the
    200 ms before it (ending a hop before it) and that 200 ms is below -55 dBFS: a spike out of digital silence;
  - not in the first second (a track starting after leading silence looks just like a pop), and only a spike, not the
    start of a sound: the 200 ms from 50 ms after it are below -55 dBFS again, unless the file ends first (so a hidden
    track starting out of silence is not a click);
  - one within 3 s of the end is "a pop at the very end";
  - clipping is a flat run of 3 or more samples at full scale (within 0.1 dB of it, and within one 16-bit step of each
    other) in any channel; more than 3 such runs (runs within 10 ms of each other being one) is clipping, told by the
    first. A loud master's odd full-scale sample, or a lossy decode's overshoot past full scale (never flat), passes.
  - Told with times, m:ss, three at most: "A click at 1:23", "2 clicks at 1:23, 3:05", "5 clicks at 0:12, 1:23, 3:05,
    …", "A pop at the very end", "Clipped at 2:41"; several at once are joined with "; ".
- **The cost**: one FFT of 2,048 every 2,048 samples, a few sums a sample, and four numbers kept a hop (about 2.8 KB a
  second of sound), on the decode the gate made anyway.
