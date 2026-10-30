# 0067: One import: what you did elsewhere, from any source, matched once, previewed, undone

- Status: accepted (the owner, 2026-10-24: "We should most definitely make these generic import functions"; "make it
  so")
- Date: 2026-10-24
- Builds on: 0030 (importing from a media server, as amended 2026-10-15), 0064 (the listening history, amended: the
  ListenBrainz import), 0051 (accounts), 0026 (undo)

## Context

Settings → Import held two one-off tools, both made for one move to Cratarr: what an account did on Jellyfin (plays,
last played, favourites, ratings, added dates; `serve::import`), and a ListenBrainz user's listens into the history
(`app::listenbrainz`). Each found tracks its own way: Jellyfin by the files both see, ListenBrainz by recording, artist
and title. Neither said what was there already or what it would change, and neither could be taken back. Someone
moving to Cratarr comes from somewhere else: another music server, a listening service, a pile of exported files.

## Decision

**One import, sources plugged in.**
- **What comes in:** listening history (each listen with its time), play counts and last played, favourites (tracks,
  albums, artists), ratings, added dates, playlists. A source brings what it has.
- **Sources:** music servers (Jellyfin and Emby; Subsonic and OpenSubsonic servers such as Navidrome, gonic and
  Airsonic; Plex later), listening services (ListenBrainz, Last.fm), and files (a ListenBrainz or Last.fm export,
  playlists as `.m3u`, `.m3u8` or `.xspf`).
- **One matcher** finds each item in the library, in this order: the file, where the source sees the same files; the
  MusicBrainz recording, where both know it; artist and title (letters and digits, any case); album and title. Length
  breaks a tie. What is not found is counted and listed, never guessed.
- **Preview first, always:** what would come in and what was not found ("4,210 of 4,388 found; 178 not in your
  library", with the list), before anything is written.
- **Again and again:** running an import again brings only what is new (0030's rules for counts, stars and ratings; a
  listen's own id for the history), never doubles.
- **Undone as a whole:** an import's undo takes out what it brought in: its listens, the plays it added, the stars and
  ratings it set where they are still as it set them. Added dates (always the earlier one) stay.
- **Whose:** everyone imports into their own account; an administrator can import into any.

**Settings → Import** is laid out as Connections: a row per import (its source, when it last ran, what it brought),
each opening its own page: its fields, Preview, Import, and Undo once it has run. New kinds of source come with an
"Add import" row once there is more than one way in.

**Built in this order:** (1) the matcher, preview and undo, with Jellyfin and ListenBrainz on them; (2) Subsonic
servers, the most common source people move from; (3) Last.fm; (4) files.

## Consequences

- `app::import` holds the matcher and the shared shape of a preview and a result; each source reads what it has and
  hands it over.
- A listen imported from a service is logged as heard, finished, not counted (a server's import brings the counts);
  its context names the source, which is how its undo finds it.
- The ListenBrainz import moves from its own page group into an import of its own, previewed first.

## Built (2026-10-24)

- **Step 1.** `app::import` holds the matcher (`Matcher`) and what an import says (`Tally`). ListenBrainz has a preview
  and an undo (its listens out of the history, found by their context). A server's import has an undo
  (`DELETE /api/subsonic/import`): the plays it added off the song, its album and artists, its stars and ratings away
  where they are still its own, and what it remembered forgotten. Settings → Import: a row per import, each its page.
- **Checked against beets** (the owner: "compare it with beets"): the same order as its Last.fm import (recording, then
  title with artist or album). Taken from its string distance as a second, looser key: accents folded, "&" as "and",
  brackets, a "feat." part and a leading "the" left out. Not taken: its substring fallback, which finds the wrong
  "Intro". Albums are found by name and album artist, then loosely, then by name where only one album has it.
- **Step 2, Subsonic servers.** "Add import" asks a server's address and login; the import connects for the run only
  (`subsonic:<address>` as the source, the password given each run and kept nowhere) and finds tracks by name, so the
  server needs no real paths and is not one of Cratarr's sources. Its address, login and choices are kept for next
  time. The Subsonic API shows the plays and stars of the account that logs in, so each person imports their own.
- **Step 3, Last.fm** (`app::lastfm`). Its API (`user.getRecentTracks`, `user.getLovedTracks`) with the API key the
  person makes at Last.fm, given once and kept per Cratarr account (never shown again). Scrobbles go into the history
  as ListenBrainz's do (context `lastfm`, not counted); one the history has from elsewhere already (the same song within
  a minute: a scrobbler often fed ListenBrainz too) is not added twice. Loved tracks become favourites kept as an
  import's (source `lastfm`), so a track loved no more there is unstarred here and the undo takes back only what is
  still its own. Its listens, like ListenBrainz's, are not where the history's own log starts (`heard_from`).
- **Step 4, files** (`app::import_files`; `POST /api/import/file?name=…&preview=…` with the file as the body, up to
  256 MB, kept nowhere). Histories: ListenBrainz's export (its zip, deflated files inflated with `flate2`, or its
  JSON lines), Spotify's (extended streaming history, or account data; under 30 s is no listen, as Spotify counts it),
  Last.fm's CSV as export tools write it. Their listens go into the history with context `file:<kind>`, a listen there
  already from anywhere not added twice (one routine for every import that brings listens, `import::into_history`);
  undone per kind. Playlists: `.m3u`/`.m3u8` (`#EXTINF` "Artist - Title", else the path's folders and file name),
  `.xspf`, Exportify's CSV; each becomes a new playlist of the tracks found ("a playlist from these tracks"), named as the
  file names it and free among one's own, undone as any playlist made is.
