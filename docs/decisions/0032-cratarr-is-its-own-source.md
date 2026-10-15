# 0032: Cratarr is its own source

- Status: accepted
- Date: 2026-10-15

## Context

Cratarr reads music through sources (0005): a connected server (Jellyfin) tells it the libraries and their folders,
the albums, the tracks and their ids, the favourites, what plays, and takes playlists and commands. Since 0030 Cratarr
serves the OpenSubsonic API from its own library (built from the files' tags, with its own ids and its own user data),
and since 0031 it has its own players (the Android app, Play On). The owner wants Cratarr to use its own library now,
without Jellyfin for music. With the Jellyfin source switched off, the pages found no server with a Music library, as
everything in them reads through a connected source.

## Decision

Cratarr has a built-in source of its own, the kind `cratarr`, answering what a server answered from what Cratarr itself
holds. The rest of Cratarr (playlists, Incoming, the Collection, the players and the pages) keeps reading through
sources as before.

- **Libraries** are the folders Cratarr sees (the media mount, 0009); Settings → Libraries gives them their uses
  (Music, Incoming) as it did a server's libraries.
- **Albums, tracks, covers** come from Cratarr's own library (0030), with its ids (`tr-…`, `al-…`). The catalogue links
  those ids to files as it links any source's, so a file is found by them (`/api/file/cratarr/tr-…`).
- **Favourites and play data** are Cratarr's own user data (0030): stars, plays.
- **Players and what plays** are Play On's (0031): the Android app, commanded by Cratarr itself.
- **No server playlists**: the playlists are the `.m3u` files, as always. With no server to keep copies on, the sync of
  0014 has nothing to do; Subsonic apps see the playlists themselves (0030).

Another source (Jellyfin, a Subsonic server) can still be added beside it; Jellyfin stays for video.

## Consequences

- Finamp, the Jellyfin web player and Home Assistant's Jellyfin automations no longer get Cratarr's playlist changes
  once Jellyfin is off for music; they move to Cratarr (Subsonic apps, the Android app, Play On) or stay with what
  Jellyfin last had.
- The ids the pages and the players use change from Jellyfin's to Cratarr's: a queue held over the change is dropped.
