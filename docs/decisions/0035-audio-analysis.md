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
- **Cratarr's decoder cannot read most of the library yet** (2026-10-16). Music holds 23,521 MP3 and 982
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
  blank in airplane mode.
- **Order**: the 0034 server half, then the radio (0036), then this, unless the owner reorders.
- **Work split:**
  - the pass, the table and the endpoint (catalogue);
  - the seekbars (the web player's NowPlaying, the app), the players' levelling, the Opus copies' gain
    tags;
  - the OpenSubsonic fields (`bpm`, `replayGain`), and reading the files' gain tags into the catalogue.

## Amendment, 2026-10-17: whether a decode was whole

Of the whole library's first pass, 19 files decoded short. Twelve are one album's FLACs that are damaged: the
reader walks the whole stream, but only 7 to 19 seconds of each decode, with Symphonia and with ffmpeg alike (ffmpeg's
reported time is the last timestamp it saw, not decoded sound). Seven are MP3s that ffmpeg decodes in full and
Symphonia does not, one stopping at 3:32 of 8:19 where its format changes. A short decode's length, loudness, tempo
and waveform describe only that part. An analysis now says whether its decode covered at least 97% of what the reader
walked (the end of its last packet; not the header's length, which is what lies for other files). Older analyses read
as not known, and only a complete analysis serves anything (length, gain, BPM, waveform; an album waits for it). The
seven MP3s were analysed again from ffmpeg's whole decode (2026-10-17); the damaged album stays incomplete until it is
replaced.

## Amendment, 2026-10-17: beats

For the visuals (decision 0046, amended), the analysis also keeps **every beat**: taktus's `beats_of` over the onsets
it already makes, at the tempo kept (after the slow count and the dance fold), with each beat's strength (its onset
against the track's strong beats). A track without a tempo has none, kept as none.

- **Kept** in `analysis_shape.beats` (a column added at start): five bytes a beat, its time in milliseconds (u32,
  little-endian) then its strength (0–255): about 3 KB for a five-minute track at 120 BPM. NULL: not worked out (an
  analysis from before); an empty value: none. In an analysis' JSON line (the ThinkBook's pass, the import), base64.
- **No version change,** so nothing is analysed again in bulk. The NAS's pass keeps beats for what it analyses from
  now on; an older analysis gets them when they are first asked for: decoded at once (one at a time, a turn of at most
  20 s, a decode of at most 60 s) and kept beside the analysis, which stays as it is, while the file is as it was. A
  decode that does not cover the whole file gives none, kept as none, so a damaged file is not decoded at every request.
- **Served** at `GET /api/track/{source}/{id}/beats`, beside `/waveform`: the bytes as kept, an ETag of the analysis,
  kept a month; 404 for a track whose file Cratarr does not keep, 503 when they cannot be worked out now.

## Amendment, 2026-10-17: the sound in bands

For the visuals (decision 0046, amended), a track's sound in three bands (low below 150 Hz, middle 300 Hz to 2.5 kHz,
high above 5 kHz): each band's loudness every 50 ms and its transients, worked out in the decode that finds the beats,
**only when the visuals first ask** (`analyse_with_bands`; the pass never does, so the library does not grow by some
25 KB a track). Kept in `analysis_shape.bands` beside the beats, as they are (an added column, the analysis' stamp
guarding the write, upserted, following the file); a decode that does not cover the file gives none, kept as none.
Served at `GET /api/track/{source}/{id}/bands`, whose doc gives the format (versioned by its first byte). One decode
answers both routes: the page asks for both at once, and the second finds the first's result.

## Amendment, 2026-10-18: the visuals' signals (format 3)

The owner asked that every signal the music gives can steer every aspect of the show ("For each aspect of the music we
can get a signal for, I want to be able to control its effect on each of the main aspects of the visualization"), and
for all the reasonable ones but stems ("Okay yes I want all of those but not stems!"; Demucs took 333 s of four cores
for a 284 s track). The three bands become **format 3** (`analysis::signals`), still worked out only when the visuals
first ask, in the same one decode, kept in the same column (`analysis_shape.bands`, read as `signals_of`), served at
the same route; formats 1 and 2 count as not worked out and are done again when asked.

- **Six adjacent bands** on a hearing scale: sub below 60 Hz, bass 60–150, low-mid 150–400, mid 400 Hz–2 kHz,
  presence 2–5 kHz, air above 5 kHz. The three before left 150–300 Hz and 2.5–5 kHz out (a snare's body, a voice's
  presence). Through each band's filters (24 dB an octave at each edge, the usual crossover slope; a tone in the middle of
  a band reaches its neighbour about 15 dB down): its **level** (20 a second) and **crest** (peak against RMS over
  200 ms: punch against density). From a short-time Fourier transform of the sound's middle and side (46 ms windows
  every 11.6 ms): its **flux** (the rise of its log-compressed spectrum: the onset strength, Bello et al. 2005) and the
  **hits** picked from it against a moving median (Böck et al. 2012); its **width** (side against middle and side: 0
  mono, 0.5 two unrelated channels, 1 opposites); its **percussive and harmonic** parts (Fitzgerald 2010: medians
  across 17 frames and across 17 bins of a 96-bin log-frequency spectrogram, soft masks) and the percussive part's hits;
  its **swell**.
- **The whole track:** the loudness as heard (EBU R128 momentary, absolute: the show's intensity), the **spectral
  centroid**, **chroma** and the **harmonic change** (Harte et al. 2006: the tonal centroid's movement) with its peaks,
  the sections' **novelty** (Foote 2000: a Gaussian checkerboard kernel along a beat-by-beat self-similarity of chroma
  and timbre, as large as fits on both sides alike) with its **boundaries**, each beat's **place in its bar** (of 3 or 4,
  where the low bands' starts fall hardest; with how sure: good on 4/4 with a kick on one, poor on jazz, classical and
  free time, and a page ignores it when unsure), the **swell** (the loudness rising now, by at least 2 dB over 1.5 s,
  more than 3 dB since its low; a single hit is none) and **build-ups** (a swell that brightens and leads into a
  boundary: the whole track being known, the show can build towards the drop).
- **The form:** a format byte and the number of sections, then self-describing sections (an id, a kind: series, hits,
  marks or the band table; a rate, channels, a count, a scale id), so a later signal is another section, not another
  format; a page skips the sections it does not know. The route's doc lists them.
- **Above 48 kHz** the sound is taken down to 44.1 or 48 kHz first (two low-passes at 20 kHz): nothing the show is
  steered by lies above, and a 192 kHz file took five times as long a minute as a CD's.
- **Measured on the NAS** (`cratarr-server --signals <files>`, 2026-10-18): a 4:41 CD track 3.8 s and 254 KB; a 6:50
  track at 96 kHz 7.9 s; a 7:07 track at 192 kHz 11.7 s (18.0 before the take-down); a 43-minute track 32.5 s and
  2.2 MB: about 52 KB a minute, within the 60 s decode, kept only for tracks played with the visuals or looked ahead to.
- **Look-ahead:** the next two tracks of each playing or paused queue get theirs before they play, on a free turn only
  (a page waiting for a decode comes first), on every change of a player and once a minute. Until a track's are there,
  a browser player listens to the sound itself (`web/src/lib/visuals/live.ts`: the band signals and the loudness from
  running statistics); the app's player is not the page's, and waits for the server.
