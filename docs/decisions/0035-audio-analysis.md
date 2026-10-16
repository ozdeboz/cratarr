# 0035: Audio analysis: waveform, tempo and loudness, worked out once on the server

- Status: proposed
- Date: 2026-10-16
- Builds on: 0013 (the converter's decode), 0030 (OpenSubsonic), 0032 (Cratarr is its own source)

## Context

The owner asked how Symfonium draws its waveform seekbar (a row of bars following a track's loudness), and then
whether the same work could give the tempo (BPM).

What Symfonium does, from its developer's own words and settings (researched 2026-10-16):

- It works the waveform out **on the phone**, on demand. It fetches the whole file a second time and decodes it,
  apart from playback, and keeps the result on the phone. For server files this is off by default, and Wi-Fi only.
- The developer refuses to do this for a whole library: "a mobile application not a media server".
- The same data drives its Smart Fades.

Other servers:

- Jellyfin, Navidrome and OpenSubsonic offer no waveform.
- Plex works out loudness and "sonic analysis" on the server, in its maintenance window, and Plexamp's waveform needs
  that analysis first.

What it costs:

- Measured on the PC with FFmpeg, decoding a 4-minute track takes 0.2 s (FLAC 16/44.1) to 0.5 s (FLAC 24/96,
  MP3 320). symphonia is within about 15 % of FFmpeg.
- On the NAS that is roughly 0.5–1.5 s per track, or 4–10 CPU-hours for about 30,000 tracks. Reading every file once
  takes 1.5–2 hours.
- Working out the values from decoded audio costs little next to decoding it.
- **Cratarr's decoder cannot read most of the library yet** (claude-5f, 2026-10-16). Music holds 23,521 MP3 and 982
  FLAC files, but symphonia is built with `flac`, `wav`, `aiff`, `pcm`, `isomp4` and `alac` only: the converter never
  needed MP3. The FFmpeg figures above are mostly MP3 320, so they must be measured again with symphonia's MP3
  decoder on the NAS (N95) before "overnight" is promised.

What the files carry today: Cratarr reads the BPM tag (`songs.bpm`), but only 33 of Music's 24,503 tracks have one,
and 353 of Incoming's 5,184. That is why the crate builder's BPM range finds almost nothing.

Working this out in a browser would need the whole file downloaded beside the stream, and about 85 MB of decoded
audio held in memory for a 4-minute track. On a phone that is not workable.

## Decision (proposed)

**One background analysis pass on the server**, in the catalogue.

- It decodes each track once (symphonia, which gains its `mp3` feature: the same crate and licence, MPL-2.0) and
  works out everything below from that one decode.
- It runs on one thread at the lowest priority: nice 19, set on the thread itself (setpriority on its tid), or
  SCHED_IDLE.
- It yields to every other heavy job: a conversion (it never runs during one), the Opus copies (the COPIES
  semaphore), and a running library import.
- The first fill is an overnight job. After that it covers new and changed tracks after each scan, keyed on the
  file's size and modification time.
- Keep and Move (0013, 0034) reuse the decode they already do for conversion.
- A track asked for before its turn is worked out at once (about a second) and kept. This on-demand path has its own
  limit of one decode at a time, so several players opening at once cannot stack decodes.
- **Keyed by Cratarr's track id**, the file's size and modification time, and the algorithm's version.
  - A Keep or Move keeps the track id and changes the file. Its row is written under the moved file's new size and
    modification time, so the reused decode counts for the file as it now is.

**What it keeps per track**, in a catalogue table of its own (`analysis`: the track, the algorithm's version, then the
values below). It is never written into the files.

- **Waveform:** 256 values, one byte each. Each value is the RMS loudness of its stretch of the track, mapped from
  −60…0 dBFS to 0–255. RMS, not peaks: a loud modern master drawn from peaks is a flat block.
  - That is about 8 MB for the whole library.
  - A player reduces the 256 values to the bars it draws.
- **Tempo:** a BPM with a confidence, or **no tempo**. Beatless and ambient tracks, a good part of this library,
  get no tempo rather than a guess.
  - **A proven estimator is ported, not invented** (the owner's rule: copy, don't reinvent):
    - librosa's tempo (ISC licence);
    - or Mixxx's QM-DSP TempoTrackV2 (GPL-2.0-or-later, which AGPL allows).
  - It is chosen after both are measured on a sample of tracks whose tempo the owner knows. That measurement counts
    the half- and double-tempo mistakes every detector makes.
- **Loudness**, for volume levelling the way ReplayGain does (the owner, 2026-10-16: "replay gain and waveform
  analysis and BPM analysis will all be grouped"): integrated loudness and true peak (EBU R128), by the `ebur128` crate
  (MIT), the reference implementation, in the same decode.
  - **Track gain and album gain.** Track gain levels shuffle and radio; album gain keeps an album's own dynamics (a
    quiet interlude stays quiet). A player uses album gain while an album plays in order, else track gain.
  - **Album gain is measured, not averaged.** The pass walks the library album by album and keeps each track's
    measurement until the album's last, then gates them together (`loudness_global_multiple`), as a ReplayGain
    scanner does. An album with a changed track is measured again whole. A track worked out on demand gets its track
    gain at once; its album's waits for the pass.
  - **Target −18 LUFS** (ReplayGain 2.0's reference): gain = −18 − loudness. The peaks are kept beside the gains
    (linear, as ReplayGain's tags), so a player never raises a track past its true peak.
- **Each value keeps where it came from:** this pass, the file's tags, or an import (below).

**Tempo from three places, in this order:**

1. the file's own BPM tag;
2. AudioMuse's tempo, once the radio's analysis is imported (decision 0036, phase 2: Essentia's RhythmExtractor,
   better than anything written here);
3. this pass's estimate.

**Loudness from two places:** this pass's; until a track has one, the file's own tags (`REPLAYGAIN_TRACK_GAIN` and
`_PEAK`, `REPLAYGAIN_ALBUM_*`; Opus's `R128_TRACK_GAIN` and `R128_ALBUM_GAIN`, which count from −23 LUFS, so 5 dB is
added). iTunes's `iTunNORM` is not read. The pass wins once it has a value, so every track is levelled the same way.

**Where it shows:**

- `GET /api/track/{source}/{id}/waveform`, beside the other track routes (`/details`, `/playlists`) and resolved
  through `track_of_source`, so Incoming's tracks have one too. It returns the 256 bytes, with an ETag and a long cache
  life.
  - The web player and the app draw the seekbar from it.
  - A Cratarr OpenSubsonic extension may offer it to Subsonic apps.
- **BPM:** by the order above, the analysis's only when confident.
  - It fills the crate builder's BPM range.
  - It fills OpenSubsonic's `bpm` on songs, which Symfonium and other apps show and filter on.
- **Loudness:** OpenSubsonic's `replayGain` on songs (`trackGain`, `albumGain`, `trackPeak`, `albumPeak`), so other
  apps level too.
  - Cratarr's own players level by it: the web player with a gain node, the app in its player (Media3).
  - The Opus copies (0033's downloads) carry it as `R128_TRACK_GAIN` and `R128_ALBUM_GAIN`, so offline playback
    levels too.

## Measured: MP3 with Cratarr's decoder on the NAS (spike, 2026-10-16)

A standalone build of symphonia =0.6.1 with the `mp3` feature added, doing per file what the pass does with the sound
(decode it whole, a waveform's bins at 1/100 s, EBU R128 loudness with the sample peak), single-threaded at nice 19 /
idle I/O on the N95, files read from the disks. 170 random Music files: 150 MP3, 20 FLAC.
- **MP3: 0.78 s per track** (average track 5 min), about 360 times real time. **FLAC: 1.28 s per track.** The slowest,
  a 40-minute track: 6.6 s. Decode errors: 4 bad packets in 150 files (skipped, the rest decodes).
- **The whole of Music** (23,521 MP3, 982 FLAC): about **5.5 hours**; Incoming's 5,184 tracks about 1 more. An
  overnight first pass holds, for decode and loudness. The tempo estimator's own cost is still to be measured.
- **13 % of the MP3s (20 of 150) do not open** with symphonia's probe: "unsupported audio codec", once "no suitable
  format reader". All 20 are ordinary MPEG-1 Layer III behind an ID3v2 tag (one of 2 MB, a large cover). Started
  after the tag (its header's size, plus a footer when flagged), **all 20 decode** at the same speed with no bad
  packets. The pass must do the same: on a probe failure, skip an ID3v2 tag and probe again (the spike reads the file
  from the tag's end through a small `MediaSource` that hides the offset).

## Open questions for the owner

1. **Tempo shown only when confident**, with "no tempo" otherwise, or always a number? **Answered (2026-10-16): only
   when confident.**
2. **Accuracy check before tempo is used:** the owner names about 20 tracks with a known tempo, a mix of styles, to
   measure against.

## Where the first pass runs (the owner, 2026-10-16)

**The one-time pass over the whole library runs on the ThinkBook**; the NAS then keeps up with new and replaced files.
The NAS could do it alone overnight (about 6.5 h of work at the lowest priority, measured above), so the ThinkBook is
a speed-up, and the NAS's own pass is the fallback when the ThinkBook is not at hand.

- The same code as a mode of the server's binary (`cratarr-server --analyse <folder> --out <file>`), run on the
  ThinkBook over the Music share (SMB), every core busy.
- Its results are keyed by the file (its path in the library, its size and modification time) and imported into
  Cratarr's table on the NAS, which analyses only what is not there or has changed since.

## Consequences

- The ThinkBook does the one-time pass (the NAS overnight when it cannot), then the NAS a little work per new track.
- The seekbar, a populated BPM axis in crates, and levelling come with no work on the phone.
- **Offline:** the app fetches each download's waveform with it and keeps it in its store, or the seekbar would be
  blank in airplane mode (claude-5f).
- **Order** (claude-5f): the 0034 server half, then the radio (0036), then this, unless the owner reorders.
- **Work split:**
  - claude-5f: the pass, the table and the endpoint (catalogue);
  - claude-5f: the seekbars (the web player's NowPlaying, the app), the players' levelling, the Opus copies' gain
    tags;
  - claude-a5: the OpenSubsonic fields (`bpm`, `replayGain`), and reading the files' gain tags into the catalogue.
