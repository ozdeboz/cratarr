# 0054: AudioMuse's analysis, taken in

- Status: accepted (the owner, 2026-10-19)
- Date: 2026-10-19
- Builds on: 0024 (crates), 0035 (analysis), 0036 (radio), 0046 (the visuals' signals)

## Context

AudioMuse-AI analyses the library on the owner's PC (WSL docker, its own Postgres): per track a tempo, a key and
scale, an energy, a top five of its style tags (`mood_vector`: "electronic:0.608,chillout:0.561,…"), CLAP's moods and
danceability (`other_features`, all 0 until CLAP has heard the track; CLAP turned on 2026-10-19, done around 25–26
October), and a 200-number "sounds like" vector. Its `track_server_map` matches each of its items to Cratarr's own
track id (`tr-…`, by fingerprint), so a join is exact.

The owner: "Yes, import them": energy, key, the style tags and the similarity; then CLAP's moods
and danceability too. Also: AudioMuse "does not move to the NAS"; and, asked how the data travels: a file
over SSH every half hour.

## Decision

- **The PC writes, Cratarr reads.** A scheduled task on the PC (`tools/audiomuse_export.py`, every 30 minutes while
  the PC is on) reads AudioMuse's database (one SELECT, read only) and writes JSON lines, one per track Cratarr has:
  `{"track": "tr-…", "energy", "key", "scale", "style", "moods", "embedding"}` (the vector as base64 of
  little-endian 32-bit floats). It copies the file over SSH to `<data>/audiomuse/audiomuse.jsonl` on the NAS and puts
  it in place in one move, only when it changed. No API key: Cratarr's keys never change the server's data.
- **Cratarr takes in the whole set** when the file changed (its size and time, checked every five minutes by the
  background pass): one transaction replaces what it held (`audiomuse` in the catalogue). Tracks it does not have are
  counted, not kept.
- **What shows**: a track's details carry `energy` (0..1), `key` ("D minor"), `style` (the top five tags with scores),
  `mood` (CLAP's that stand out, 0.3 and up, danceable apart), `danceability` (0..1), each null where AudioMuse has
  none (CLAP's two while all its scores are 0); and the listener's `play_count` and `last_played`. The page calls the
  tags Style and keeps Mood for CLAP's (the cover overview).
- **Crates** select on all four (the owner): Mood (CLAP's moods as values), Energy and Danceability (0–100%
  ranges), Style (the tags as values).
- **Similarity**: "sounds like" is a sixth signal of the radio's closeness (Similar songs, Radio's inserts, Autoplay's
  Similar), weighed as each of the other five, where the seed and the song were both heard: the cosine of the song's
  vector with the seed's mean vector, on 0 to 1 from 0.62 to 0.92. Measured on the 8,909 vectors (2026-10-19): two
  random tracks are 0.62 alike at the median, the same artist's other albums 0.82, one album's tracks 0.875 (90th
  percentile 0.956). A song or seed AudioMuse has not heard keeps the five signals.

## Consequences

- The PC must be on for new analysis to arrive; what arrived stays.
- About 12 MB a copy, only when AudioMuse's data changed.
- Radio or Autoplay steered by mood or energy, and CLAP's text search, are for later (the owner: "talk about it
  later"; "Yes, later").
