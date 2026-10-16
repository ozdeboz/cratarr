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

What the files carry today: Cratarr reads the BPM tag (`songs.bpm`), but only 33 of Music's 24,503 tracks have one,
and 353 of Incoming's 5,184. That is why the crate builder's BPM range finds almost nothing.

Working this out in a browser would need the whole file downloaded beside the stream, and about 85 MB of decoded
audio held in memory for a 4-minute track. On a phone that is not workable.

## Decision (proposed)

**One background analysis pass on the server**, in the catalogue.

- It decodes each track once (symphonia) and works out everything below from that one decode.
- It runs on one thread at low priority, and never while a conversion runs.
- The first fill is an overnight job. After that it covers new and changed tracks after each scan, keyed on the
  file's size and modification time.
- Keep and Move (0013, 0034) reuse the decode they already do for conversion.
- A track asked for before its turn is worked out at once (about a second) and kept.

**What it keeps per track**, in a catalogue table of its own (`analysis`: the track, the algorithm's version, then the
values below). It is never written into the files.

- **Waveform:** 256 values, one byte each. Each value is the RMS loudness of its stretch of the track, mapped from
  −60…0 dBFS to 0–255. RMS, not peaks: a loud modern master drawn from peaks is a flat block.
  - That is about 8 MB for the whole library.
  - A player reduces the 256 values to the bars it draws.
- **Tempo:** a BPM with a confidence, or **no tempo**. Beatless and ambient tracks, a good part of this library,
  get no tempo rather than a guess.
  - The estimator (onset strength, then the period that fits it best, within 60–200 BPM) is chosen after it has been
    measured on a sample of tracks whose tempo the owner knows.
  - The half- and double-tempo mistakes all detectors make are counted in that measurement.
- **Loudness** (an option for the owner): integrated loudness and true peak (EBU R128). From these follows a track
  gain for volume levelling, the way ReplayGain works.

**Where it shows:**

- `GET /api/tracks/{id}/waveform`: the 256 bytes, with an ETag and a long cache life.
  - The web player and the app draw the seekbar from it.
  - A Cratarr OpenSubsonic extension may offer it to Subsonic apps.
- **BPM:** a file's own BPM tag wins; else the analysis's, when confident.
  - It fills the crate builder's BPM range.
  - It fills OpenSubsonic's `bpm` on songs, which Symfonium and other apps show and filter on.
- **Loudness** (if taken up): OpenSubsonic's `replayGain` on songs, and the players' levelling.

## Open questions for the owner

1. **Loudness:** in this pass (it costs little more), or not now?
2. **Tempo shown only when confident**, with "no tempo" otherwise, or always a number?
3. **Accuracy check before tempo is used:** the owner names about 20 tracks with a known tempo, a mix of styles, to
   measure against.

## Consequences

- The NAS does a one-time overnight pass, then a little work per new track.
- The seekbar, a populated BPM axis in crates, and (optionally) levelling come with no work on the phone, so they
  work offline too, once the data has been fetched.
- **Work split:**
  - claude-5f: the pass, the table and the endpoint (catalogue);
  - claude-5f: the seekbars (the web player's NowPlaying, the app);
  - claude-a5: the OpenSubsonic fields (`bpm`, `replayGain`).
