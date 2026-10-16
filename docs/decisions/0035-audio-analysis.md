# 0035: Audio analysis: waveform, tempo and loudness, worked out once on the server

- Status: accepted (the owner, 2026-10-16: it ships in preview 36)
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

## Decision

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

- **Waveform:** 512 values, one byte each. Each value is the RMS of its stretch of the track, linear, as 0–255 of
  the track's loudest stretch. Its loudness is kept apart (below), so the bytes go to its shape.
  - RMS, not peaks: a loud modern master's peaks are near full scale all through, so it would be a flat block.
  - Linear, not in dB: a dB scale squeezes the shape too. Measured on four of the library's tracks, drawn as the
    seekbar draws them (120 bars against the loudest): for a loud master, the bars at 90 % or more of the top were
    87 % of them from peaks, 89 % from RMS in dB, 15 % from linear RMS. For the three others, linear RMS gave the most
    shape too (2–18 % at the top, against 4–36 % from peaks and 31–45 % in dB).
  - That is about 15 MB for the whole library.
  - A player reduces the 512 values to the bars it draws.
- **Tempo:** a BPM with a confidence, or **no tempo**. Beatless and ambient tracks, a good part of this library,
  get no tempo rather than a guess.
  - **A proven estimator is taken, not invented** (the owner's rule: copy, don't reinvent). First proposed: librosa
    or QM-DSP; taken: taktus, measured (the amendment and the measurements below). The first proposal was:
    - librosa's tempo (ISC licence);
    - or Mixxx's QM-DSP TempoTrackV2 (GPL-2.0-or-later, which AGPL allows).
  - It is chosen after both are measured on a sample of tracks whose tempo the owner knows. That measurement counts
    the half- and double-tempo mistakes every detector makes.
- **Loudness**, for volume levelling the way ReplayGain does (the owner, 2026-10-16: "replay gain and waveform
  analysis and BPM analysis will all be grouped"): integrated loudness (EBU R128), by the `ebur128` crate (MIT), the
  reference implementation, and the sample peak, in the same decode.
  - **Track gain and album gain.** Track gain levels shuffle and radio; album gain keeps an album's own dynamics (a
    quiet interlude stays quiet). A player uses album gain while an album plays in order, else track gain.
  - **Album gain is measured, not averaged.** The pass walks the library album by album and keeps each track's
    measurement until the album's last, then gates them together (`loudness_global_multiple`), as a ReplayGain
    scanner does. An album with a changed track is measured again whole. A track worked out on demand gets its track
    gain at once; its album's waits for the pass.
  - **Target −18 LUFS** (ReplayGain 2.0's reference): gain = −18 − loudness. The peaks are kept beside the gains
    (linear, as ReplayGain's tags; above 1.0 is possible from a lossy file), so a player never raises a track past
    its peak.
  - **The sample peak, not the true peak:** the largest decoded sample, as ReplayGain's peak tags have always held
    and rsgain writes by default. A true peak would oversample every decode (BS.1770's 4×).
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
  through `track_of_source`, so Incoming's tracks have one too. It returns the 512 bytes, with an ETag and a long cache
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
   measure against. **Replaced (2026-10-16)** by the published results below, the owner's two answers and our own run
   on public datasets: 20 tracks would measure accuracy to about ±21 points, too wide to add to what is published.
3. **Beatless and ambient tracks get no tempo:** fine (the owner, 2026-10-16).
4. **Dub and reggae: the slow count** (about 70, not 140; the owner, 2026-10-16).

## Amended (2026-10-16): the estimator must give a confidence; what is published

Read in the sources (Zapata & Gómez 2011; Percival & Tzanetakis 2014; Schreiber's tempo_eval reports and per-genre
results; Knees et al. 2015; Zapata et al. 2012). Acc1 is the right tempo within 4 %, Acc2 also counts half, double and
thirds:

| Estimator | Mainstream sets (Ballroom, GTZAN, ACM Mirum) | Electronic (GiantSteps) | Hard, expressive (SMC) | Confidence |
|---|---|---|---|---|
| QM-DSP TempoTrackV2 (Mixxx) | 59-67 / 88-91 | 27 / 46 | 13 / 31 | none |
| librosa (its ancestor, Ellis 2007) | about the same as QM (45.6 / 80.7 against 43.2 / 80.4 on ISMIR04) | | | none |
| Essentia's multi-feature (AudioMuse's) | no peer-reviewed tempo figures | | | yes: the trackers' agreement, 0 to 5.32 |

- **By genre** (GTZAN, QM): hip-hop 93 / 99, disco 89 / 98, reggae 40 / 99 (its misses are half or double only),
  jazz 44 / 73, classical 19 / 56 (every system, neural ones too, fails there).
- **Both of the first candidates give no confidence**, so they cannot do "only when confident" (question 1). A
  committee's agreement does: Zapata et al. 2012 found that 1.5 bits of agreement keeps about 73 % of tracks, with
  beats right 90 % of the time above it and 59 % below, and listeners agreeing.
- **Nothing published tests "no tempo"**: no standard dataset holds beatless or ambient music.

**So the candidates change:**

1. **taktus** (Rust, Apache-2.0, 2026), which reports Essentia-level accuracy and a confidence gate keeping 82-95 % of
   tracks at 97-99 % Acc2. Its figures are its own: **we run it on the public sets ourselves first** (GTZAN, Ballroom,
   GiantSteps, with tempo_eval's annotations) before taking it.
2. **Else Essentia's multi-feature agreement**, ported (AGPL-3.0, as Cratarr).

**The owner's answers as rules:**

- A tempo below the confidence threshold is no tempo. Beatless and ambient tracks are expected there, and are checked
  on the library's own ambient-tagged tracks: they should come out without one.
- **The slow count for dub and reggae:** a track whose genre tag is reggae, dub, roots or dancehall and whose estimate
  is above 110 BPM gets half of it. Checked on the library's reggae-tagged tracks: they should land between about 60
  and 95.
- **Dance music between 90 and 180** (the owner, after the measurements below): a track whose genre tag holds a
  dance word (house, techno, trance, drum and bass and the like) gets its estimate doubled or halved into 90–180. The
  slow count wins when a track has both.


## Measured: taktus on the public sets (2026-10-16)

taktus 0.1.0, decoded as Cratarr decodes (symphonia), the whole excerpt (`tempo_of`); Acc1 within 4 %, Acc2 also
half, double and thirds. Gated: `sure >= 0.05` and `catch >= 1.0`.

| Set | Acc1 | Acc2 | Gated: answers | Gated Acc1 | Gated Acc2 | Time (this PC) |
|---|---|---|---|---|---|---|
| Ballroom, 698 (Krebs' corrected beats) | 77.4 % | 99.0 % | 93.4 % | 79.1 % | 99.2 % | 0.07 s per 30 s |
| GiantSteps, 664 (corrected v2) | 80.9 % | 95.5 % | 94.3 % | 82.1 % | 97.3 % | 0.27 s per 2 min |

- **Its own figures hold**, to the decimal on both sets. Against the original 2004 Ballroom values it scores 95.6 %
  Acc2: those values have known errors, which Krebs' beat annotations correct.
- **The gate:** stricter ones gain under a point and lose a lot (sure >= 0.15, catch >= 1.5: GiantSteps answers
  62 %). The core uses 0.05 / 1.0.
- **What it misses when sure:** on GiantSteps mostly 4:3 (127 read as 168), then 2:3; octave slips (half or double)
  are 95 of 626 answers. Folding every estimate into 90–180 BPM would raise GiantSteps' gated Acc1 to 88 % but lower
  Ballroom's to 70 % (its slow dances): only for dance genre tags, if at all (the owner's call).
- **taktus is taken** (the core, `server/src/analysis`). The source was read: one file, no dependency,
  `forbid(unsafe_code)`, no file or network access.

## Measured: the two tempo rules on a real library (2026-10-16)

taktus as above, then the two rules (dub and reggae counted slow; dance tags folded into 90–180), on 586 tracks of
the owner's library on the NAS (0.6–0.9 s a track at the lowest priority, no file unreadable). The reference is the
files' own BPM tags, of unknown origin and quality: worse than the public sets' annotations.

| Sample | Tracks | Sure enough | Result without the rules | With the rules |
|---|---|---|---|---|
| BPM-tagged, any genre | 386 | 64 % | 71 % right (79 % any octave) | 71 % right |
| Dub, reggae, dancehall | 60 | 62 % | 16 % in 60–95 | 65 % in 60–95 |
| Dance tags | 80 | 81 % | 97 % in 90–180 | 100 % in 90–180 |
| Ambient | 60 | 47 % | 53 % without a tempo | the same |

- **The gate does the most:** on the tagged tracks, what it shows is 71 % right; what it holds back would have been
  51 % right.
- **The fold barely acts:** it moved 3 of the 247 tagged answers (one fixed, one broken: a track tagged "Breaks" at
  75; one wrong either way) and 2 of 65 dance answers. It is kept: cheap, and right where it acts on dance sets.
- **The slow count does what it is for:** it halves the dub read at 112–186. It leaves the ones at 95–104 (the
  rule starts above 110); whether they are right is not known: no reference.
- **Ambient:** about half gets no tempo; whether the shown half has a beat was not checked.
- Neither rule changes how often a tempo is shown; only the gate does.

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

## Amendment, 2026-10-17: whether a decode was whole

Of the whole library's first pass, 14 files decoded only in part: Symphonia's FLAC reader takes one frame in eight
of one album's files (its reader walks the whole stream, the decoder gets every eighth frame), and an MP3 whose
format changes mid-stream stops decoding there while ffmpeg plays on. Their length, loudness, tempo and waveform then
describe only that part. An analysis now says whether its decode covered at least 97% of what the reader walked (the
end of its last packet; not the header's length, which is what lies for other files). Older analyses read as not
known. Only a complete analysis' length may be served in place of the header's; the files that decode in part are
analysed again from a whole decode by another decoder.
