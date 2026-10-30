# 0065: Insights: Collection · Listening · Map

- Status: accepted (the owner, 2026-10-23: "a revision of the insights page. It's quite old … a new tab for listening
  history … new names for old tabs … redesign the whole page. The way tabs look is not nice"; chose the names, the
  tabs as pills in the overlay row ("B, but collection goes first"), Listening time and Taste over time, and the old
  history from elsewhere)
- Date: 2026-10-23
- Builds on: 0023 (Related playlists, the graph), 0064 (one listening history), 0021 (the page's look)

**Amended 2026-10-24 (the owner, for 0.8):** the tabs are gone. Insights is laid out as Settings and Health: its page
lists the views in groups (Collection: Overview, Genres, Artists, Tracks; Listening: History, Profile; Playlists:
Map), each opening in the panel (`insights:<view>`, beside the list on a roomy window, over it on a phone, the one last
open opened again), with a breadcrumb; the panel's controls (the period, the libraries, sort, sources) are a sheet from
the sliders in the view's bar. The Map in the panel opens a playlist over itself (back returns to the map).

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
Tracks: those in playlists, once Playlists). Centred while no panel is open; at the row's end beside one.

**Two choices are Insights' own, the same in every tab's panel** (the owner, 2026-10-23): the libraries (one switch
each; Collection, History, Profile and the map count only what is in those switched on) and the period (the past week,
month or year, or all: Listening over it, Collection the albums that came in it, Profile's plays and skips; not the
map, which is how the playlists relate now).

**Insights' own panel** (the owner, 2026-10-23) holds how the open tab is viewed, never the pill: Collection's libraries
(a switch each, the page's library switches as on Library and Crates; its figures count those switched on), Listening's
period, the Map's filters. Opened by a button at the row's end, closed by the panel's ✕ (top left). Beside the page at
a fixed width on a roomy window, never over it there; over the page on a phone. Each tab's controls are data of a few
kinds (lib/insightsControls), which the panel draws one way, as the settings' rows: a tab adds a control there, never
markup. All remembered.

**Collection** (the figures as charts, after the dataviz method: the form by the data's job, one hue where one series):

- the headline figures as a row of stat tiles: tracks, albums, artists, hours, size;
- a compilation's "Various Artists" is no artist: not counted, not a bar (it dwarfed every real one's, the owner);
- Tracks (the owner, 2026-10-23): in a playlist or plays, one at a time (the panel's Count, as Genres and Artists
  count): the top list (in the most playlists, most played: each the server's ranking of the whole set), and
  for plays the tracks by how often (0, 1, 2–4, 5–9, 10–19, 20+); no tiles (playlists and missing tracks are Overview's);
- Genres and Artists: every one sent, so each order (tracks, in a playlist, A–Z) is the top of the whole set;
- shares as meters, never pies: lossless albums, albums below 320 kbps, tracks in a playlist;
- decades as columns, each opening its ten years (the title a breadcrumb back);
- Overview: the headline figures; Incoming (albums waiting, how long the oldest has); Growth (albums added per step of the
  period: a week's days, a month's weeks, a year's months, all's years; from what Cratarr knows of their arrival: an album with none is not counted, nothing filled in);
  Heard (tracks played at least once, albums never played, favourites: the account's plays and stars); Listed (tracks
  in a playlist, tracks in one of the account's own crates, not Unlisted); quality and decades; genres and artists only on their own views (the owner,
  2026-10-23: the overview should not duplicate them), as bars of their tracks or of those in a playlist (the panel's
  Count: two scales and two rankings, so one at a time, the unit said over the bars);
- the playlists' figures as tiles, and the tracks in the most playlists as a list.

**Listening** has two sections in the pill's second row (the owner, 2026-10-23): History and Profile.

**Profile** is the listener's taste as Cratarr reads it, from the one similarity (`GET /api/history/profile`):
what built it (favourites, tracks in playlists, played; skipped and excluded apart), then per genre, artist, style
and playlist what there is more of (netted) and less of; the sound's numbers as where the taste sits on the library's
own range, less of under it; the moods compared with the library (a bar either way from its middle).

**History** (the log, 0064): a period (Week, Month, Year);

- **listening time:** columns per day (a week, a month) or per week (a year), the total as the headline;
- **taste over time:** within the period, as everything (the owner, 2026-10-23; at first it reached further back,
  last.fm's way): a week's days, a month's weeks, a year's months, all's years since the log began; one line per genre, its share of the time listened then, the five
  heard longest (the fixed categorical order, checked for both themes; a legend with each one's average share);
- **the top artists, albums and tracks** of the period, with how often.

Until the log reaches back, each shows what there is; nothing is made up. The log is filled from ListenBrainz, which
Jellyfin's ListenBrainz plugin scrobbled to (Jellyfin keeps no history of its own; 0064 amended).

**Map** is the graph as it is, its filters in the panel.

**Collection's views:** Overview (the tiles, the meters, the decades, the first genres and artists), Genres and
Artists (all of them), Tracks (those in playlists).

**Read once:** `GET /api/history/listening?period=week|month|year` (app/history.rs, the log's only reader).
`GET /api/insights?source=&without=<source>/<library>` (Collection from the libraries switched on).
