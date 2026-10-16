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
- **Loudness** (an option for the owner): integrated loudness and true peak (EBU R128), by the `ebur128` crate (MIT),
  the reference implementation. From these follows a track gain for volume levelling, the way ReplayGain works.
- **Each value keeps where it came from:** this pass, the file's tags, or an import (below).

**Tempo from three places, in this order:**

1. the file's own BPM tag;
2. AudioMuse's tempo, once the radio's analysis is imported (decision 0036, phase 2: Essentia's RhythmExtractor,
   better than anything written here);
3. this pass's estimate.

**Where it shows:**

- `GET /api/track/{source}/{id}/waveform`, beside the other track routes (`/details`, `/playlists`) and resolved
  through `track_of_source`, so Incoming's tracks have one too. It returns the 256 bytes, with an ETag and a long cache
  life.
  - The web player and the app draw the seekbar from it.
  - A Cratarr OpenSubsonic extension may offer it to Subsonic apps.
- **BPM:** by the order above, the analysis's only when confident.
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
- The seekbar, a populated BPM axis in crates, and (optionally) levelling come with no work on the phone.
- **Offline:** the app fetches each download's waveform with it and keeps it in its store, or the seekbar would be
  blank in airplane mode (claude-5f).
- **Order** (claude-5f): the 0034 server half, then the radio (0036), then this, unless the owner reorders.
- **Work split:**
  - claude-5f: the pass, the table and the endpoint (catalogue);
  - claude-5f: the seekbars (the web player's NowPlaying, the app);
  - claude-a5: the OpenSubsonic fields (`bpm`, `replayGain`).
