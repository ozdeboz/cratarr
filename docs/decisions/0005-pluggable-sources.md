# 0005: Music sources are plugins; libraries are chosen, paths are found

- Status: accepted; library roles revised by [0009](0009-library-uses.md)
- Date: 2026-10-12

## Context

Cratarr was written against Jellyfin, with the library given as path prefixes (`CRATARR_MUSIC_PREFIXES`) and the
playlist folder's place on Jellyfin as another path. Both turned out to be fragile: Jellyfin reports track paths as
`/data/media/Music/...` while its library settings say `/Volume1/media/Music`, so neither can be derived from
Jellyfin's library settings. The owner wants other servers to be possible later and, when several are in use,
Cratarr to work with all of them (a bridge), with a picker in the page. Syncing playlists into servers that do not
read `.m3u` files is a separate feature and is not decided here.

## Decision

- A source is a plugin compiled into the binary: a module implementing the `Source` trait (`source/mod.rs`),
  listed in `KINDS`. No dynamic loading. Optional abilities have default implementations (the Albums page's track
  details). Syncing, when it comes, gets its own trait rather than widening this one.
- A plugin declares its settings as fields (`url`, `text`, `secret`, `choice`, `multi_choice`) and fills the
  choices from the live server. The Settings page draws every plugin's form from that. A secret names the fields
  it is bound to, and the core (not the plugin) refuses to reuse a saved secret once one of them changes. This
  generalises 0004's address/key rule to every plugin. A contract test checks every plugin's fields.
- The core owns everything about playlist files. A source only reports a track's library id and its path as the
  source sees it. The core finds the playlist folder's place on each source by stripping leading folders off sample
  track paths until the rest is a file in the playlist folder (`placement.rs`). No path is configured.
- Library roles per source: `music` (chosen ids, or empty for every library inside the playlist folder) and an
  optional `incoming`. Incoming tracks never go in playlists, even if the library lies inside the folder (no longer so: see the amendment of 2026-10-15). A library
  cannot have both roles. `GET /api/status` says whether any source has an incoming library, so the page can show
  incoming-music features only then.
- Several sources may be on. The page sends `?source=` (chosen per browser in the source picker), or none:
  then the source whose track started most recently wins. Playlist changes are passed to every source. A source
  that fails does not hide the others; only when every source fails is it an error.
- Jellyfin specifics: library membership comes from the track's ancestors, never from paths. The player to follow
  is an app, optionally on one device, matched by name: device ids are not stable (observed for Finamp), and a
  session's client name can differ slightly from the device list's ("Jellyfin for Android" vs "Jellyfin Android").
- No Jellyfin user is configured. The `.m3u` playlists are household-wide: every user with access to the Music
  library sees all of them (observed: all six users see all 67), and Jellyfin's playlist ownership and sharing do
  not apply to imported playlists (`GET /Playlists/{id}` and `/Playlists/{id}/Users` answer 400). Every call
  Cratarr makes works with the API key alone, and the connection check uses the authenticated `/System/Info`, except
  one: which library a track is in. Libraries are per-user views in Jellyfin's API (without a user, a track's
  ancestors are physical folders, and `ParentId` is ignored once `Ids` is given), so that call runs as Jellyfin's
  first administrator, found once; an administrator sees every library, so which one does not matter.
  Per-user playlists need Jellyfin's own playlists and a login in Cratarr; they are left to that future work. The
  parked Albums page loses per-user play counts and favourites until then.
- `settings.json` becomes `{"version": 2, "sources": [...]}` and, once present, is the whole truth. Without it, one
  Jellyfin source comes from `JELLYFIN_*` (first run). A file in 0004's flat format is read as that source.
  `CRATARR_MUSIC_PREFIXES`, `CRATARR_JELLYFIN_PLAYLIST_DIR` and `JELLYFIN_USER_ID` are retired (a warning if set).
- Startup no longer fails when a source is down. Its status shows on the page, and its libraries are looked for
  again every minute. An unreadable playlist folder still stops Cratarr.

## Consequences

- Adding a server means one module and a line in `KINDS`; no UI or settings-format work.
- Music only counts if its source can see it inside the playlist folder. Servers with music elsewhere (level 2 in
  the design talk: different folders per server) are not supported, because playlist lines are relative paths and
  Cratarr refuses `..` and absolute ones.
- Once saved from the page, the environment no longer configures sources; deleting `settings.json` brings it back.
- Every Settings page load asks each source for its status, choices and library samples (about 0.3 s with one
  Jellyfin source).

## Amendment (2026-10-15): Incoming tracks go in playlists too

The owner decided that a track in Incoming can be added to a playlist like any other. A playlist made in Finamp could
already bring Incoming tracks in through the sync (decision 0014), while Cratarr's own menus refused them; now both
work the same way.

- An Incoming track's line is its path from the Music folder ("../lidarr-music/X/01.flac"), the form the sync and the
  album actions already used. The catalogue resolves such lines (`Catalogue::place_of_line`): a playlist's rows, its
  play, and its sync all see the track.
- Keep and Replace move the line along with the file into Music; Delete takes it out of every playlist, and Undo puts
  it back (decision 0011 §6, unchanged).
- A Shared copy holds such a line as it is; the media server finds the file as long as it can see that folder.
