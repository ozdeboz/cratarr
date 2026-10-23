# 0065: Insights: Collection · Listening · Map

- Status: accepted (the owner, 2026-10-23: "a revision of the insights page. It's quite old … a new tab for listening
  history … new names for old tabs … redesign the whole page. The way tabs look is not nice"; chose the names, the
  tabs as pills in the overlay row ("B, but collection goes first"), Listening time and Taste over time, and the old
  history from elsewhere)
- Date: 2026-10-23
- Builds on: 0023 (Related playlists, the graph), 0064 (one listening history), 0021 (the page's look)

## Context

Insights had two tabs, Statistics and Related, as a segmented control under the page's overlay row: the back button
above them, a row of their own. Statistics was lines of figures; Related the playlists' graph. There was nothing about
listening, and from 0064 there is a log of every track heard.

## Decision

**Three tabs, named for the question each answers:** Collection (what you have: today's Statistics), Listening (what
you heard: new), Map (how your playlists relate: today's Related). The tab last open is remembered.

**The tabs sit in the overlay row,** a pill group beside the back button where other pages have their pill (lib/TopPill),
in the same glass; the chosen one filled, never touching the back button. A tab's own sections are a second row attached
below, inside the pill, which grows to hold it (the owner, 2026-10-23): Collection's (Overview, Genres, Artists,
Playlists). Centred while no panel is open; at the row's end beside one.

**Insights' own panel** (the owner, 2026-10-23) holds how the open tab is viewed, never the pill: Collection's libraries
(a switch each, the page's library switches as on Library and Crates; its figures count those switched on), Listening's
period, the Map's filters. Opened by a button at the row's end, closed by the panel's ✕ (top left). Beside the page at
a fixed width on a roomy window, never over it there; over the page on a phone. Each tab's controls are data of a few
kinds (lib/insightsControls), which the panel draws one way, as the settings' rows: a tab adds a control there, never
markup. All remembered.

**Collection** (the figures as charts, after the dataviz method: the form by the data's job, one hue where one series):

- the headline figures as a row of stat tiles: tracks, albums, artists, hours, size;
- a compilation's "Various Artists" is no artist: not counted, not a bar (it dwarfed every real one's, the owner);
- shares as meters, never pies: lossless albums, albums below 320 kbps, tracks in a playlist;
- decades as columns, genres and artists as bars (one series each, the accent, labelled directly);
- the playlists' figures as tiles, and the tracks in the most playlists as a list.

**Listening** (the log, 0064): a period (Week, Month, Year);

- **listening time:** columns per day (a week, a month) or per week (a year), the total as the headline;
- **taste over time:** further back than the period, as last.fm shows it (the owner): the past 12 weeks (a week
  chosen), 12 months (a month) or 5 years (a year); one line per genre, its share of the time listened then, the five
  heard longest (the fixed categorical order, checked for both themes; a legend with each one's average share);
- **the top artists, albums and tracks** of the period, with how often.

Until the log reaches back, each shows what there is; nothing is made up. The log is filled from ListenBrainz, which
Jellyfin's ListenBrainz plugin scrobbled to (Jellyfin keeps no history of its own; 0064 amended by a5).

**Map** is the graph as it is, its filters in the panel.

**Collection's views:** Overview (the tiles, the meters, the decades, the first genres and artists), Genres and
Artists (all of them), Playlists.

**Read once:** `GET /api/history/listening?period=week|month|year` (app/history.rs, the log's only reader).
`GET /api/insights?source=&without=<source>/<library>` (Collection from the libraries switched on).
