# 0012: Playlists become the user's own Jellyfin playlists, kept in step with the .m3u files both ways

- Status: proposed
- Date: 2026-10-12
- Builds on: decision 0011 (track identity, events). Changes decision 0002.

## Context

Cratarr is used by one person. Its playlists are `.m3u` files in the Music folder, which Jellyfin imports as
playlists visible to every account with access to the library (six on the reference install). Jellyfin's own
ownership and sharing do not apply to imported `.m3u` playlists. The owner wants the playlists to be theirs.

Checked on the reference install (Jellyfin 10.11, read-only):

- A playlist can be created for one user with `IsPublic: false` (private), and shared later with chosen users.
- Its whole track list can be replaced in one request, with the ids in the body (no query-string limit: the
  5,041-track playlist fits).
- Jellyfin skips folders whose name starts with a dot: the `.m3u` backups in `Music/.cratarr-backups` never became
  playlists (0 of 67).

## Decision

- Settings asks, per server, whose playlists these are: a Jellyfin user, chosen from the server's users.
- The playlist module (decision 0011) holds each playlist as an ordered list of track ids. It keeps two copies in
  step:
  - the `.m3u` file, moved into a dot-folder inside Music so Jellyfin no longer imports it. It stays portable and is
    what export and backups use;
  - the chosen user's private Jellyfin playlist.
- In step both ways. A change made in Cratarr is written to both copies. A change made in Jellyfin (in Finamp, say) is
  read back on the background pass, by comparing playlists' change stamps, and merged into the `.m3u`. The merge is
  three-way, against the last state both copies agreed on:
  - added on either side: added;
  - removed on either side: removed;
  - reordered on one side: that order;
  - the same spot changed on both sides: the newer change wins.

  A missing track (a line whose file is gone) has no Jellyfin id. It stays in the `.m3u` and is never treated as
  removed.
- Incoming tracks may be in playlists. Adopting the album keeps them (same track ids, decision 0011). Rejecting it
  takes them out of both copies, and Undo puts them back.
- Only playlists Cratarr tracks are synced. Jellyfin playlists made by others (the weekly ListenBrainz ones) are
  left alone.
- Sync state (which `.m3u` goes with which Jellyfin playlist, and the last agreed list) lives in the data folder,
  next to the catalogue and the journal.

## Migration, once

1. Move the 67 `.m3u` files into the dot-folder and rewrite their lines for the new location.
2. Create a private Jellyfin playlist per file for the chosen user.
3. The imported playlists disappear from Jellyfin, for every account.

## Consequences

- Every playlist gets a new Jellyfin id. Whatever refers to the old ids needs them again: Home Assistant's voice
  playlist list (68 entries), the Chill playlist id in the welcome-home automation, and playlists downloaded in
  Finamp for offline use.
- Other Jellyfin accounts no longer see these playlists, unless they are shared with them.
- The data folder now holds state that cannot be rebuilt (catalogue track ids, the journal, the sync state), so it
  must be backed up.
