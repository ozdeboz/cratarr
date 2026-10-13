# 0014: Sync links: one playlist, the targets it is kept in

- Status: accepted
- Date: 2026-10-13
- Supersedes: the "Playlists belong to" switch of 0012 (its sync engine stays)
- Builds on: 0002 (playlists are `.m3u` files), 0007 (export), 0012 (own Jellyfin playlists)

## Context

0012 made every playlist the owner's private Jellyfin playlist at once, with one switch for the whole server. The
owner decided on 2026-10-12, with both sessions, that this is a per-playlist choice:

- A playlist can be *global*, visible to everyone, or *user-bound*: one account's own playlist.
- Syncing is a link per playlist to a target, with its own direction, conflict rule and status.
- One playlist can have several targets, set up from its menu ("Sync to…").
- Export (0007) stays a one-off download; a sync link lasts.

## Decision

- **One source:** every playlist is an `.m3u` file in `Music/.playlists` (lines carry `../`, as in 0012). It never
  moves; a target is a copy of it.
- **A sync link** is a playlist plus a target, a direction and a conflict rule, with a status (last sync, error).
  Links live in the sync state in the data folder.
- **Targets:**
  - **Shared:** a copy in the Music folder, which the media server imports for every user. Push only, because an
    imported playlist cannot be edited through Jellyfin (its API answers 403). The copy is rewritten whenever the
    playlist changes, and removed when the link goes.
  - **A user's Jellyfin playlist:** that account's own playlist, kept in step by 0012's engine (three-way merge,
    renames, the guards against mass deletion).
  - Later: a folder (a USB stick, another device's share) and other servers, as further kinds of target.
- **Direction:** push (the file wins; changes on the target are overwritten) or two-way. For two-way, the
  conflict rule picks the side that leads when both changed: the newest change (0012's merge), the file, or the
  target.
- **New on a target:** a playlist made in Finamp by an account that has a two-way link becomes a new playlist in
  Cratarr, with a two-way link to it.
- **Gone on one side:** a playlist deleted in Cratarr loses all its targets' copies. A copy deleted on a two-way
  target deletes the playlist (it can be restored in Cratarr). On a push target the copy is made again.
- **Restored:** a deleted playlist's links are put aside, not forgotten. Restoring it (its Undo) brings them all
  back, and the next pass makes the copies again. A new playlist of the same name starts fresh, Shared.
- **Taking a link off** deletes its copy on the target.
- **Migration, self-contained:** on the first pass after deploying, every `.m3u` in Music is copied into
  `.playlists` and gets a Shared link. The file in Music stays, as that link's copy: same path, same content, so
  nothing visible changes and the media server keeps the playlist (and its id) it imported.
  This runs once, recorded in the sync state, so the copies are never mistaken for playlists again.

## Consequences

- The owner chooses per playlist: global (Shared), user-bound (a user's Jellyfin playlist), both, or neither.
- Home Assistant's voice lookup finds playlists by name: a playlist with a user's Jellyfin target has a new id
  there.
- The data folder holds the links: they belong in its backup (as 0012 already says).
