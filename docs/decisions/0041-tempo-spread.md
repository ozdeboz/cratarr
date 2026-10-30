# 0041: The tempo spread

- Status: accepted
- Date: 2026-10-17

## Context

Since decision 0035 most tracks have a tempo: the BPM tag, else the analysis' when it is confident, none for a track
without a beat. The owner asked what profiling a playlist by it could look like, after what others do (researched
2026-10-17): Paul Lamere's Sort Your Music and Organize Your Music (a sortable table, a scatter to lasso tracks from),
Chosic's averages, Plexamp's sonic paths, Navidrome's and Jellyfin's BPM-range smart playlists, beets' running
"trainings", DJ tools' energy arcs and flow. Most asked for: a BPM range (running, workouts, dance classes); most
loved: seeing the shape, not one average (a study of 985 sleep playlists found six kinds hiding behind one mean).
The traps everyone meets: half or double time, and tracks without a beat shown as 0.

A sketch on Echo Garden (472 tracks, 218 with a tempo) showed three views: tempo in playlist order, the same wider,
and the spread. The owner chose the spread as the most informative.

## Decision

**The spread:** how many tracks fall in each 5-BPM band, from 60 to 180 BPM, always that scale, so two spreads compare
at a glance. Bars grow upward from a baseline in the waveform's look (lib/Waveform: thin rounded bars, muted, the chosen
part in the accent). Tracks without a tempo are not a bar: the count of those with one says how many it shows. Tempos
are counted as stored: dub and reggae at their slow count (0035).

**Where it shows:**
- In a playlist's Info, as it is: only shown, not a control. Nothing else becomes a filter by a tap (the owner: no
  secret filters).
- In the filter sheet of a list of tracks, as the tempo selector: the spread over two handles, the bars inside the
  range lit, the number of tracks it keeps beside it. For that list only and not remembered, as Hide missing tracks.
- Later, the crate editor's BPM rule as the same selector, its spread counted over the library (its axis needs the
  bands from the server).

## Consequences

- No new data for a playlist: its rows carry their tempo.
- A list with few tempos shows a thin spread; the count says so.
- Loudness can have a spread of its own later; rows do not carry it yet.

## Amended (2026-10-17, the owner): in the header of every panel with many tracks

Mock-ups on real data (a playlist, a crate, a genre, an artist, three playlists together) settled it:

- **Placement:** the spread sits in the panel's header, under its genres, for a playlist, a crate (genres too) and an
  artist. Not for an album, whose ten-odd tracks make no spread worth reading. Info keeps its own.
- **Several playlists selected:** one stacked spread, each playlist in its colour. Five colours, then one grey layer
  for the rest. No legend: the colour shows as a dot on each playlist's section below, which already names it (the
  owner).
- **Counts:** missing tracks are not counted, so its line agrees with the header's.
- **A crate:** its spread is counted on the server (`GET /api/crates/{id}/tempo`, as its favourites toggle is set),
  since a genre holds thousands of tracks and its rows come a page at a time. The counting is one pure function
  (`crates::tempo_bands`), for the crate editor's BPM axis later.
