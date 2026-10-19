# 0049: The visuals' beat, downbeat and tempo from a local pulse

- Status: accepted, amended 2026-10-19 (Beat this! directly; proposed by claude-5f, 2026-10-18; the owner asked for it after "drum hits are sometimes recognized while no
  beat signal is showing", and for the web's authoritative insight on better beat and downbeat signals, "possibly
  utilizing other signals we already have")
- Date: 2026-10-18
- Builds on: 0035 (tempo: taktus and its confidence gate, the beats amendment), 0046 (the visuals' signals)

## Context

The visuals' Beat, Downbeat and Tempo rows all come from decision 0035's beats: taktus estimates one tempo for the
whole track; if its confidence passes the gate (`sure >= 0.05`, `catch >= 1.0`) a beat grid is laid over the whole
track at that tempo, each beat with a strength (the strongest onset at it); otherwise the track has no beats at all.
The owner's observations follow from that:

- **A third of the library has no beats.** The gate gives a tempo for 64 % of the BPM-tagged tracks and about half of
  the ambient ones (0035, measured on 586 tracks). It is right for the BPM column, where "no tempo" is the honest
  answer, but the visuals use it too: such a track gets no Beat, no Downbeat and a Tempo of 90 BPM, while Drum hits
  (a separate detector, every track) keep firing. Of the 11 calibration tracks, three have none, a deep house track
  ("A Midnight Swim", Cosmic Handshakes) among them.
- **Where there is a grid, it does not stop.** Across ambient, trip-hop, techno and indie rock, no gap between beats is
  longer than 1.5 beats: the grid runs through breaks at the track's tempo, with weak beats there (trip-hop: a run of
  33 beats under 0.15 strength, about 16 s). So Tempo stays up where nothing beats, and Beat barely shows where the
  drums play off the grid.

## What is known (read 2026-10-18)

- **Learned trackers are the state of the art for both beats and downbeats.** A recurrent network with a dynamic
  Bayesian network over bars (Böck, Krebs, Widmer, ISMIR 2016; madmom). "Beat this!" (Foscarin, Schlüter, Widmer,
  ISMIR 2024) drops the DBN, whose meter and tempo constraints fail on tempo and meter changes, and surpasses it in
  F1; it still fails on hard, underrepresented genres. MIT (code and weights; some training audio is copyrighted),
  beats and downbeats, a small model of about 8 MB (ONNX conversions exist).
- **Downbeats need more than rhythm.** Rhythm-only methods rely on percussion and do not carry across styles; Durand,
  Bello, David and Richard combine harmony, bass, melody and rhythm features, the harmonic change around the bar line
  being a key cue (ICASSP 2016; IEEE/ACM TASLP 2017).
- **A local pulse, not a global grid.** The Predominant Local Pulse (Grosche and Müller, ISMIR 2009; IEEE TASLP 2011)
  fits, window by window, the strongest periodicity in the onset novelty and adds the fitted sinusoids up: it follows
  gradual and sudden tempo changes, and works only "as long as the underlying novelty function possesses locally
  periodic patterns", so where there is no pulse it fades. Pulse clarity (Lartillot et al., DAFx 2008) is the
  perceptual name for that strength; a DAFx 2024 paper separates pulse strength from pulse stability.
- **The percussive part makes a cleaner rhythm novelty** (FitzGerald, DAFx 2010; percussive detectors find hi-hats
  between kick and snare that energy detectors miss).
- **Dynamic programming over a global tempo** (Ellis 2007, librosa's ancestor) follows tempo changes of about ±10 %
  only.

## Proposal

For the visuals only; the BPM column keeps 0035's gate.

1. **A local pulse for every track.** PLP over an onset novelty weighted towards the percussive part and the drum
   hits (signals the analysis already makes), about a five-second window, 30–300 BPM. It gives:
   - the beats, at the pulse's peaks, where it is strong enough;
   - a local tempo, beat by beat;
   - a pulse strength (0 in a break or beatless music), which the Tempo row multiplies in and the Beat row's
     strength follows, so both go quiet where the pulse does.
   Where taktus's gate passes, its tempo picks the pulse level (no octave slips against the BPM shown).
2. **Downbeats from what is already measured.** For each bar phase (which of every four beats is the first),
   score the bass bands' hits and the harmonic change at those beats over a few bars, and take the best; held while
   the pulse holds, found again after a break. Durand's harmony and bass cues, without drums needed.
3. **Beat this! as the reference, not (yet) as the analysis.** Run on a sample of the library (the beatless three,
   the ambient-tagged, the dance-tagged, plus a random draw), as 0035 measured taktus: beat F-measure and downbeat
   F-measure of 1 and 2 against it, and where 1 and 2 put no pulse, whether it does. Take it into the analysis only if
   1 and 2 fall clearly short, and then after measuring its CPU time on the NAS (it would add an ONNX runtime).

## Consequences

- The library is analysed again for the visuals' signals (a version step; the BPM column is untouched).
- Beat, Downbeat and Tempo describe what is heard: a pulse where there is one, none where there is not.
- Nothing changes for the governor's or the thumbs' calibration until the new signals are in; then the calibration is
  run again.
- Interim (before this lands, if the owner wants it): the Tempo row fades with the recent beats' strength and is 0
  without beats; the turning keeps the grid's tempo so the flame does not stand still in ambient passages.

## Open questions for the owner

- Take 1 and 2 with the measurement in 3, or go to Beat this! directly?
- The interim Tempo fade in 64.1, or wait for this?

## Amendment (2026-10-19): Beat this! directly, and the interim now

The owner answered both (in a5's session, after the proposal was explained): **"Beat this! directly"**, not 1 and 2
measured against it first; and the interim Tempo fade now.

- **The analysis is Beat this!**: beats and downbeats from its model, run on the NAS through an ONNX runtime (the
  server's part, claude-5f). Its CPU time on the NAS is measured on a sample first, so the library's pass is planned
  from a real figure; 1 and 2 above are dropped, the reference sample in 3 stays as the check that it works here.
- **The interim, in 70** (the page's part, a5: `lib/visuals/music.svelte.ts`): the Tempo row is the tempo times a
  smoothstep of the last bar's mean beat strength, 0 at 0.15 or less (the break figure) and all of it at 0.35 or more
  (provisional: measured by a5 on 12 Test-crate tracks, a steady bar's mean is 0.3 to 0.85; at 0.35 the grooves are
  full 69 to 95% of the time and the breaks reach 0); 0 without beats or outside the grid, moving across each beat so
  it never jumps. The
  turning keeps the grid's tempo (`bps` untouched), so the flame does not stand still in ambient passages.

## Sources

- Böck, Krebs, Widmer, Joint Beat and Downbeat Tracking with Recurrent Neural Networks, ISMIR 2016:
  https://archives.ismir.net/ismir2016/paper/000186.pdf
- Foscarin, Schlüter, Widmer, Beat this! Accurate beat tracking without DBN postprocessing, ISMIR 2024:
  https://arxiv.org/pdf/2407.21658, code and weights https://github.com/CPJKU/beat_this
- Durand, Bello, David, Richard, Robust Downbeat Tracking Using an Ensemble of Convolutional Networks:
  https://ar5iv.labs.arxiv.org/html/1605.08396
- Grosche, Müller, tempogram and PLP, ISMIR 2009:
  https://www.audiolabs-erlangen.com/content/05_fau/professor/00_mueller/03_publications/2009_GroscheMueller_Tempogram_ISMIR.pdf,
  and the FMP notebook https://www.audiolabs-erlangen.de/resources/MIR/FMP/C6/C6S3_PredominantLocalPulse.html
- Online PLP, pulse strength and stability, DAFx 2024: https://dafx.de/paper-archive/2024/papers/DAFx24_paper_23.pdf
- Lartillot et al., pulse clarity, DAFx 2008: https://dafx.de/paper-archive/details/ykOTkZckZEzq0rncQJTGEA
- FitzGerald, harmonic/percussive separation, DAFx 2010:
  https://www.audiolabs-erlangen.com/resources/aps-w23/papers/2010_FitzGerald_HarmonicPercussiveSep_DAFx.pdf
- Ellis, Beat Tracking by Dynamic Programming, 2007:
  https://hajim.rochester.edu/ece/sites/zduan/teaching/ece472/reading/Ellis_2007.pdf
